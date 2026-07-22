use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::canonical_json::hash_bytes;
use crate::docs::section::{ANCHOR_VERSION, CHUNK_VERSION, EXTRACTOR_VERSION};
use crate::storage::atomic::atomic_replace;
use crate::{PulseError, PulseResult};

pub const CACHE_SCHEMA_VERSION: u32 = 1;
pub const ENGINE_MODE: &str = "lexical";
pub const ENGINE_NAME: &str = "tantivy";
pub const EXTRACTOR_NAME: &str = "pulse-markdown-sections";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EngineState {
    pub mode: String,
    pub name: String,
    pub version: String,
}

impl EngineState {
    pub fn current() -> Self {
        Self {
            mode: ENGINE_MODE.to_string(),
            name: ENGINE_NAME.to_string(),
            version: crate::docs::lexical::TANTIVY_COMPAT_VERSION.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExtractorState {
    pub name: String,
    pub version: u32,
    pub anchor_version: u32,
    pub chunk_version: u32,
}

impl ExtractorState {
    pub fn current() -> Self {
        Self {
            name: EXTRACTOR_NAME.to_string(),
            version: EXTRACTOR_VERSION,
            anchor_version: ANCHOR_VERSION,
            chunk_version: CHUNK_VERSION,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GenerationDocument {
    pub document_revision: u64,
    pub path: String,
    pub content_hash: String,
    pub section_count: u32,
    pub chunk_count: u32,
    pub body_indexed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct GenerationCounts {
    pub registered: u32,
    pub eligible: u32,
    pub indexed: u32,
    pub sections: u32,
    pub chunks: u32,
    pub excluded: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GenerationState {
    pub schema_version: u32,
    pub generation_id: String,
    pub fingerprint: String,
    pub engine: EngineState,
    pub extractor: ExtractorState,
    pub config_hash: String,
    pub registry_retrieval_hash: String,
    pub documents: BTreeMap<String, GenerationDocument>,
    pub sections_file_hash: String,
    pub projection_hashes: BTreeMap<String, String>,
    pub counts: GenerationCounts,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheState {
    Current,
    Missing,
    Stale,
    Corrupt,
    Incompatible,
}

impl CacheState {
    pub const fn as_str(self) -> &'static str {
        match self {
            CacheState::Current => "current",
            CacheState::Missing => "missing",
            CacheState::Stale => "stale",
            CacheState::Corrupt => "corrupt",
            CacheState::Incompatible => "incompatible",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedGeneration {
    pub state: GenerationState,
    pub generation_path: PathBuf,
    pub sections_path: PathBuf,
    pub tantivy_path: PathBuf,
}

pub fn cache_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".pulse/cache/docs-search")
}

pub fn current_pointer_path(repo_root: &Path) -> PathBuf {
    cache_dir(repo_root).join("CURRENT")
}

pub fn generation_dir(repo_root: &Path, generation_id: &str) -> PathBuf {
    cache_dir(repo_root).join("generations").join(generation_id)
}

pub fn builds_dir(repo_root: &Path) -> PathBuf {
    cache_dir(repo_root).join("builds")
}

pub fn is_valid_generation_id(id: &str) -> bool {
    id.starts_with("gen_sha256_")
        && id.len() == "gen_sha256_".len() + 64
        && id["gen_sha256_".len()..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

pub fn generation_id_for_fingerprint(fingerprint: &str) -> PulseResult<String> {
    let Some(hex) = fingerprint.strip_prefix("sha256:") else {
        return Err(PulseError::validation(
            "docs_index_invalid_fingerprint",
            "fingerprint must be sha256:<hex>",
        ));
    };
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PulseError::validation(
            "docs_index_invalid_fingerprint",
            "fingerprint must contain 64 hex digits",
        ));
    }
    Ok(format!("gen_sha256_{}", hex.to_ascii_lowercase()))
}

pub fn read_current(repo_root: &Path) -> Option<String> {
    let path = current_pointer_path(repo_root);
    let text = fs::read_to_string(path).ok()?;
    let id = text.trim();
    if is_valid_generation_id(id) {
        Some(id.to_string())
    } else {
        None
    }
}

fn read_current_raw(repo_root: &Path) -> PulseResult<Option<String>> {
    let path = current_pointer_path(repo_root);
    match fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text.trim().to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(PulseError::io(&path, error)),
    }
}

pub fn validate_generation(
    repo_root: &Path,
    generation_id: &str,
) -> PulseResult<ValidatedGeneration> {
    if !is_valid_generation_id(generation_id) {
        return Err(PulseError::validation(
            "docs_index_corrupt",
            "invalid docs-search generation id",
        ));
    }
    let generation_path = generation_dir(repo_root, generation_id);
    let state_path = generation_path.join("state.json");
    let sections_path = generation_path.join("sections.jsonl");
    let tantivy_path = generation_path.join("tantivy");
    let state: GenerationState = crate::storage::read_json(&state_path).map_err(|e| {
        PulseError::validation("docs_index_corrupt", format!("invalid state.json: {e}"))
    })?;
    if state.schema_version != CACHE_SCHEMA_VERSION {
        return Err(PulseError::validation(
            "docs_index_incompatible",
            "unsupported docs-search state schema",
        ));
    }
    if state.generation_id != generation_id {
        return Err(PulseError::validation(
            "docs_index_corrupt",
            "generation id mismatch in state.json",
        ));
    }
    if state.engine != EngineState::current() || state.extractor != ExtractorState::current() {
        return Err(PulseError::validation(
            "docs_index_incompatible",
            "unsupported docs-search engine/extractor version",
        ));
    }
    let sections_bytes = fs::read(&sections_path).map_err(|e| PulseError::io(&sections_path, e))?;
    if hash_bytes(&sections_bytes) != state.sections_file_hash {
        return Err(PulseError::validation(
            "docs_index_corrupt",
            "sections.jsonl hash mismatch",
        ));
    }
    if !tantivy_path.is_dir() {
        return Err(PulseError::validation(
            "docs_index_corrupt",
            "missing tantivy directory",
        ));
    }
    // Verify the engine can open the directory.
    crate::docs::lexical::open_index(&tantivy_path).map_err(|e| {
        PulseError::validation("docs_index_corrupt", format!("tantivy open failed: {e}"))
    })?;
    Ok(ValidatedGeneration {
        state,
        generation_path,
        sections_path,
        tantivy_path,
    })
}

pub fn classify(repo_root: &Path) -> PulseResult<(CacheState, Option<ValidatedGeneration>)> {
    let Some(current) = read_current_raw(repo_root)? else {
        return Ok((CacheState::Missing, None));
    };
    if !is_valid_generation_id(&current) {
        return Ok((CacheState::Corrupt, None));
    }
    match validate_generation(repo_root, &current) {
        Ok(valid) => Ok((CacheState::Current, Some(valid))),
        Err(e) if e.code() == "docs_index_incompatible" => Ok((CacheState::Incompatible, None)),
        Err(_) => Ok((CacheState::Corrupt, None)),
    }
}

pub fn classify_against(
    repo_root: &Path,
    expected_fingerprint: &str,
) -> PulseResult<(CacheState, Option<ValidatedGeneration>)> {
    let (state, valid) = classify(repo_root)?;
    let Some(valid) = valid else {
        return Ok((state, None));
    };
    if state == CacheState::Current && valid.state.fingerprint != expected_fingerprint {
        Ok((CacheState::Stale, Some(valid)))
    } else {
        Ok((state, Some(valid)))
    }
}

pub fn open_reader_generation(repo_root: &Path) -> PulseResult<Option<ValidatedGeneration>> {
    let first = read_current(repo_root);
    let Some(id) = first else {
        return Ok(None);
    };
    match validate_generation(repo_root, &id) {
        Ok(valid) => Ok(Some(valid)),
        Err(_) => {
            let second = read_current(repo_root);
            if second.as_deref() != Some(&id) {
                if let Some(second) = second {
                    return validate_generation(repo_root, &second).map(Some);
                }
            }
            Err(PulseError::validation(
                "docs_index_corrupt",
                "current docs-search generation is invalid",
            ))
        }
    }
}

pub fn publish_current(repo_root: &Path, generation_id: &str) -> PulseResult<()> {
    fs::create_dir_all(cache_dir(repo_root))
        .map_err(|e| PulseError::io(cache_dir(repo_root), e))?;
    let mut bytes = generation_id.as_bytes().to_vec();
    bytes.push(b'\n');
    let current_path = current_pointer_path(repo_root);
    // CURRENT is the visibility boundary for readers: atomic_replace syncs the
    // temp file before rename and best-effort syncs the cache directory after
    // rename. We then sync the published name again so the pointer publication
    // does not intentionally outrun the already-synced generation directory.
    atomic_replace(&current_path, &bytes).map(|_| ())?;
    sync_file(&current_path)?;
    sync_directory_best_effort(&cache_dir(repo_root))?;
    Ok(())
}

fn sync_file(path: &Path) -> PulseResult<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| PulseError::io(path, e))
}

fn sync_directory_best_effort(path: &Path) -> PulseResult<()> {
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
            // Directory sync is unavailable on some platforms/filesystems.
            // atomic_replace already synced file content and attempted the
            // parent directory, so unsupported directory fsync is best-effort.
            Ok(())
        }
        Err(error) => Err(PulseError::io(path, error)),
    }
}

#[derive(Debug)]
pub struct DocsSearchWriteLock {
    lock_path: PathBuf,
    file: File,
}

impl DocsSearchWriteLock {
    pub fn acquire(repo_root: &Path) -> PulseResult<Self> {
        Self::acquire_with_timeout(repo_root, Duration::from_secs(10))
    }

    pub fn acquire_with_timeout(repo_root: &Path, timeout: Duration) -> PulseResult<Self> {
        let locks = repo_root.join(".pulse/runtime/locks");
        fs::create_dir_all(&locks).map_err(|e| PulseError::io(&locks, e))?;
        let lock_path = locks.join("docs-search.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| PulseError::io(&lock_path, e))?;
        let start = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => {
                    return Ok(Self { lock_path, file });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() >= timeout {
                        return Err(PulseError::LockTimeout { lock_path, timeout });
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(PulseError::io(&lock_path, e)),
            }
        }
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for DocsSearchWriteLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

pub fn cleanup_generations(repo_root: &Path, _keep_current: bool) -> PulseResult<()> {
    let cache = cache_dir(repo_root);
    let builds = builds_dir(repo_root);
    if builds.exists() {
        fs::remove_dir_all(&builds).map_err(|e| PulseError::io(&builds, e))?;
    }
    let current = read_current(repo_root);
    let gens = cache.join("generations");
    if !gens.exists() {
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(&gens)
        .map_err(|e| PulseError::io(&gens, e))?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    entries.reverse();
    let previous_to_keep = entries
        .iter()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .find(|id| Some(id) != current.as_ref());
    for entry in entries {
        let Some(id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if Some(&id) == current.as_ref() || Some(&id) == previous_to_keep.as_ref() {
            continue;
        }
        fs::remove_dir_all(entry.path()).map_err(|e| PulseError::io(entry.path(), e))?;
    }
    Ok(())
}
