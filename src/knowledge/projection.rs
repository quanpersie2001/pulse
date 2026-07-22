use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonical_json::{hash_bytes, hash_value};
use crate::knowledge::manifest::KnowledgeManifest;
use crate::knowledge::model::{Learning, LearningStatus};
use crate::knowledge::relation::{KnowledgeRelation, RelationType};
use crate::{PulseError, Result};

pub const KNOWLEDGE_PROJECTION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeSnapshot {
    pub schema_version: u32,
    pub code: String,
    pub knowledge_fingerprint: String,
    pub entries: Vec<Learning>,
    pub relations: Vec<KnowledgeRelation>,
    pub inverse: InverseIndex,
    pub eligibility: EligibilityProjection,
    pub counts: KnowledgeCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct InverseIndex {
    pub derived_from: BTreeMap<String, Vec<String>>,
    pub corroborated_by: BTreeMap<String, Vec<String>>,
    pub contradicted_by: BTreeMap<String, Vec<String>>,
    pub supersedes: BTreeMap<String, Vec<String>>,
    pub promotions: BTreeMap<String, Vec<String>>,
    pub implemented_by: BTreeMap<String, Vec<String>>,
    pub applied_to: BTreeMap<String, Vec<String>>,
    pub causes: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EligibilityProjection {
    pub future_default_search: FutureDefaultSearch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FutureDefaultSearch {
    pub eligible: Vec<String>,
    pub excluded: Vec<EligibilityExclusion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EligibilityExclusion {
    pub id: String,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeCounts {
    pub entries: usize,
    pub relations: usize,
    pub by_status: BTreeMap<String, usize>,
    pub by_kind: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeStatusReport {
    pub schema_version: u32,
    pub code: String,
    pub manifest: String,
    pub knowledge_fingerprint: Option<String>,
    pub counts: KnowledgeCounts,
    pub cache_state: CacheState,
    pub errors: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheState {
    Missing,
    Current,
    Stale,
    Corrupt,
    Incompatible,
}

pub fn knowledge_fingerprint(repo_root: &Path, manifest: &KnowledgeManifest) -> Result<String> {
    let mut entries = Vec::new();
    collect_hashes(repo_root, ".pulse/knowledge/entries", &mut entries)?;
    let mut relations = Vec::new();
    collect_hashes(repo_root, ".pulse/knowledge/relations", &mut relations)?;
    let payload = json!({
        "fingerprint_version": 1,
        "manifest_hash": hash_value(manifest)?,
        "learning_schema_hash": manifest.learning_schema.sha256,
        "relation_schema_hash": manifest.relation_schema.sha256,
        "entries": entries,
        "relations": relations,
    });
    hash_value(&payload)
}

pub fn build_snapshot(
    repo_root: &Path,
    manifest: &KnowledgeManifest,
    mut entries: Vec<Learning>,
    mut relations: Vec<KnowledgeRelation>,
) -> Result<KnowledgeSnapshot> {
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    relations.sort_by(|a, b| a.id.cmp(&b.id));
    let mut inverse = InverseIndex::default();
    for relation in &relations {
        let target = format!("{}:{}", relation.to.kind.slug(), relation.to.id);
        match relation.relation_type {
            RelationType::DerivedFrom => inverse
                .derived_from
                .entry(target)
                .or_default()
                .push(relation.from.id.clone()),
            RelationType::Corroborates => inverse
                .corroborated_by
                .entry(relation.to.id.clone())
                .or_default()
                .push(relation.from.id.clone()),
            RelationType::Contradicts => inverse
                .contradicted_by
                .entry(target)
                .or_default()
                .push(relation.from.id.clone()),
            RelationType::SupersededBy => inverse
                .supersedes
                .entry(relation.to.id.clone())
                .or_default()
                .push(relation.from.id.clone()),
            RelationType::PromotedTo => inverse
                .promotions
                .entry(target)
                .or_default()
                .push(relation.from.id.clone()),
            RelationType::ImplementedBy => inverse
                .implemented_by
                .entry(relation.to.id.clone())
                .or_default()
                .push(relation.from.id.clone()),
            RelationType::AppliedTo => inverse
                .applied_to
                .entry(relation.to.id.clone())
                .or_default()
                .push(relation.from.id.clone()),
            RelationType::CausedBy => inverse
                .causes
                .entry(relation.to.id.clone())
                .or_default()
                .push(relation.from.id.clone()),
        }
    }
    normalize_inverse(&mut inverse);
    let eligibility = eligibility(&entries);
    let counts = counts(&entries, &relations);
    Ok(KnowledgeSnapshot {
        schema_version: KNOWLEDGE_PROJECTION_SCHEMA_VERSION,
        code: "ok".to_string(),
        knowledge_fingerprint: knowledge_fingerprint(repo_root, manifest)?,
        entries,
        relations,
        inverse,
        eligibility,
        counts,
    })
}

pub fn write_snapshot_cache(repo_root: &Path, snapshot: &KnowledgeSnapshot) -> Result<()> {
    let path = repo_root.join(".pulse/cache/knowledge.snapshot.json");
    let bytes = crate::canonical_json::to_canonical_bytes(snapshot)?;
    crate::storage::atomic_write(&path, &bytes)
}

pub fn cache_state(repo_root: &Path, fingerprint: &str) -> CacheState {
    let path = repo_root.join(".pulse/cache/knowledge.snapshot.json");
    if !path.exists() {
        return CacheState::Missing;
    }
    match crate::storage::read_json::<KnowledgeSnapshot>(&path) {
        Ok(snapshot) if snapshot.schema_version != KNOWLEDGE_PROJECTION_SCHEMA_VERSION => {
            CacheState::Incompatible
        }
        Ok(snapshot) if snapshot.knowledge_fingerprint == fingerprint => CacheState::Current,
        Ok(_) => CacheState::Stale,
        Err(_) => CacheState::Corrupt,
    }
}

fn collect_hashes(
    repo_root: &Path,
    relative: &str,
    output: &mut Vec<(String, String)>,
) -> Result<()> {
    let dir = repo_root.join(relative);
    if !dir.exists() {
        return Ok(());
    }
    let mut files = fs::read_dir(&dir)
        .map_err(|e| PulseError::io(&dir, e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| PulseError::io(&dir, e))?;
    files.sort_by_key(|e| e.path());
    for file in files {
        let path = file.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let bytes = fs::read(&path).map_err(|e| PulseError::io(&path, e))?;
            output.push((
                path.strip_prefix(repo_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                hash_bytes(&bytes),
            ));
        }
    }
    Ok(())
}

fn eligibility(entries: &[Learning]) -> EligibilityProjection {
    let mut eligible = Vec::new();
    let mut excluded = Vec::new();
    for entry in entries {
        match entry.status {
            LearningStatus::Candidate => excluded.push(exclusion(&entry.id, "learning_candidate")),
            LearningStatus::Disputed => excluded.push(exclusion(&entry.id, "learning_disputed")),
            LearningStatus::Superseded => {
                excluded.push(exclusion(&entry.id, "learning_superseded"))
            }
            LearningStatus::Retired => excluded.push(exclusion(&entry.id, "learning_retired")),
            LearningStatus::Reviewed | LearningStatus::Validated | LearningStatus::Promoted => {
                eligible.push(entry.id.clone())
            }
        }
    }
    EligibilityProjection {
        future_default_search: FutureDefaultSearch { eligible, excluded },
    }
}

fn exclusion(id: &str, reason: &str) -> EligibilityExclusion {
    EligibilityExclusion {
        id: id.to_string(),
        reason_codes: vec![reason.to_string()],
    }
}

pub fn counts(entries: &[Learning], relations: &[KnowledgeRelation]) -> KnowledgeCounts {
    let mut by_status = BTreeMap::new();
    let mut by_kind = BTreeMap::new();
    for entry in entries {
        *by_status
            .entry(
                serde_json::to_value(entry.status)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| format!("{:?}", entry.status)),
            )
            .or_insert(0) += 1;
        *by_kind
            .entry(
                serde_json::to_value(entry.kind)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| format!("{:?}", entry.kind)),
            )
            .or_insert(0) += 1;
    }
    KnowledgeCounts {
        entries: entries.len(),
        relations: relations.len(),
        by_status,
        by_kind,
    }
}

fn normalize_inverse(inverse: &mut InverseIndex) {
    for values in inverse
        .derived_from
        .values_mut()
        .chain(inverse.corroborated_by.values_mut())
        .chain(inverse.contradicted_by.values_mut())
        .chain(inverse.supersedes.values_mut())
        .chain(inverse.promotions.values_mut())
        .chain(inverse.implemented_by.values_mut())
        .chain(inverse.applied_to.values_mut())
        .chain(inverse.causes.values_mut())
    {
        let set: BTreeSet<_> = values.drain(..).collect();
        *values = set.into_iter().collect();
    }
}
