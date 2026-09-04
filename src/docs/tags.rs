use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::canonical_json::to_canonical_bytes;
use crate::storage::atomic::atomic_replace;
use crate::storage::WriteGuard;
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct TagsRegistry {
    #[serde(default)]
    pub tags: Vec<String>,
}

pub fn tags_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".pulse/docs/tags.json")
}

pub fn load(repo_root: &Path) -> PulseResult<TagsRegistry> {
    let path = tags_path(repo_root);
    if !path.exists() {
        return Ok(TagsRegistry::default());
    }
    let registry: TagsRegistry = crate::storage::read_json(&path)?;
    validate_registry_shape(&registry)?;
    Ok(registry)
}

pub fn bootstrap(repo_root: &Path) -> PulseResult<bool> {
    let path = tags_path(repo_root);
    if path.exists() {
        return Ok(false);
    }
    let bytes = to_canonical_bytes(&TagsRegistry::default())?;
    crate::storage::create_new(&path, &bytes)?;
    Ok(true)
}

pub fn add(repo_root: &Path, tag: &str) -> PulseResult<TagsRegistry> {
    let _guard = WriteGuard::acquire(repo_root)?;
    let tag = normalize_tag(tag)?;
    let path = tags_path(repo_root);
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        }
        let bytes = to_canonical_bytes(&TagsRegistry::default())?;
        crate::storage::create_new(&path, &bytes)?;
    }
    let mut registry = load(repo_root)?;
    if !registry.tags.contains(&tag) {
        registry.tags.push(tag);
        registry.tags.sort();
        let bytes = to_canonical_bytes(&registry)?;
        atomic_replace(&tags_path(repo_root), &bytes)?;
    }
    Ok(registry)
}

pub fn validate_tags(repo_root: &Path, tags: &[String]) -> PulseResult<()> {
    let registry = load(repo_root)?;
    for tag in tags {
        let normalized = normalize_tag(tag)?;
        if !registry.tags.contains(&normalized) {
            return Err(PulseError::validation(
                "docs_tag_unknown",
                format!("tag is not registered: {tag}"),
            ));
        }
    }
    Ok(())
}

pub fn normalize_tag(tag: &str) -> PulseResult<String> {
    let tag = tag.trim().to_ascii_lowercase();
    if tag.is_empty()
        || tag.len() > 80
        || !tag
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err(PulseError::validation(
            "docs_tag_invalid",
            "tags must be non-empty lowercase slugs",
        ));
    }
    Ok(tag)
}

pub fn validate_registry_shape(registry: &TagsRegistry) -> PulseResult<()> {
    let mut sorted = registry.tags.clone();
    sorted.sort();
    sorted.dedup();
    if sorted != registry.tags || registry.tags.iter().any(|tag| normalize_tag(tag).is_err()) {
        return Err(PulseError::validation(
            "docs_tags_invalid",
            "tags must be unique, sorted lowercase slugs",
        ));
    }
    Ok(())
}
