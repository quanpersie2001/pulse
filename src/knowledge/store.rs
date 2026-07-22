use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::{event_path, EventEnvelope};
use crate::id::{new_event_id, parse_numeric};
use crate::knowledge::manifest::{bootstrap_unlocked, KnowledgeBootstrapOutcome};
use crate::knowledge::model::*;
use crate::knowledge::projection::{
    build_snapshot, cache_state, counts, knowledge_fingerprint, write_snapshot_cache, CacheState,
    KnowledgeSnapshot, KnowledgeStatusReport,
};
use crate::knowledge::relation::*;
use crate::knowledge::validate::{
    load_records, validate_knowledge, validate_learning_for_mutation, validate_loaded,
    validate_public_learning_claims, validate_sha256,
};
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, commit_prepared_transaction,
    prepare_multi_target_transaction, prepare_transaction, recover_prepared_transactions,
    FileState, MultiTargetTransactionIntent, TransactionFailpoint, TransactionIntent,
    TransactionTarget,
};
use crate::storage::WriteGuard;
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MutationStatus {
    Created,
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationOutcome<T> {
    pub schema_version: u32,
    pub code: String,
    pub status: MutationStatus,
    pub knowledge_fingerprint: String,
    pub value: T,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationOutcome {
    pub schema_version: u32,
    pub code: String,
    pub status: MutationStatus,
    pub relation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOutcome<T> {
    pub schema_version: u32,
    pub code: String,
    pub items: Vec<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningShow {
    pub schema_version: u32,
    pub code: String,
    pub learning: Learning,
    pub relations: Vec<KnowledgeRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationAdd {
    pub relation_type: RelationType,
    pub to_kind: EndpointKind,
    pub to: String,
    pub target_revision: Option<u64>,
    pub target_hash: Option<String>,
    pub expected_revision: u64,
}

#[derive(Debug, Clone)]
pub struct OperationContext {
    pub actor: String,
    pub now: DateTime<Utc>,
}

impl Default for OperationContext {
    fn default() -> Self {
        Self {
            actor: "human:unknown".to_string(),
            now: Utc::now(),
        }
    }
}

pub struct KnowledgeStore {
    repo_root: PathBuf,
    failpoint: Option<TransactionFailpoint>,
}

impl KnowledgeStore {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            failpoint: None,
        }
    }
    #[cfg(any(test, debug_assertions))]
    pub fn with_failpoint(repo_root: impl Into<PathBuf>, failpoint: TransactionFailpoint) -> Self {
        Self {
            repo_root: repo_root.into(),
            failpoint: Some(failpoint),
        }
    }

    pub fn bootstrap(&self) -> PulseResult<KnowledgeBootstrapOutcome> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        bootstrap_unlocked(&self.repo_root)
    }

    pub fn create(
        &self,
        draft: LearningDraft,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Learning>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        let manifest = bootstrap_unlocked(&self.repo_root)?.manifest;
        recover_prepared_transactions(&self.repo_root)?;
        let (entries, mut relations) = load_records(&self.repo_root)?;
        for target in &draft.provenance_targets {
            if target.relation != RelationType::DerivedFrom {
                return Err(PulseError::validation(
                    "knowledge_relation_direction_invalid",
                    "initial provenance targets must be derived_from",
                ));
            }
        }
        if draft.provenance_targets.is_empty() && draft.source_commits.is_empty() {
            return Err(PulseError::validation(
                "learning_provenance_missing",
                "candidate requires provenance target or source commit",
            ));
        }
        for commit in &draft.source_commits {
            crate::source::resolve_full_commit(&self.repo_root, commit).map_err(|_| {
                PulseError::validation(
                    "knowledge_relation_endpoint_missing",
                    format!("source commit does not resolve: {commit}"),
                )
            })?;
        }
        let id = self.allocate_id()?;
        if entries.contains_key(&id) || self.entry_path(&id).exists() {
            return Err(PulseError::AlreadyExists { subject: id });
        }

        let mut new_relations = Vec::new();
        for target in &draft.provenance_targets {
            let relation = KnowledgeRelation::new(
                RelationType::DerivedFrom,
                id.clone(),
                Endpoint {
                    kind: target.kind,
                    id: target.id.clone(),
                    revision: target.revision,
                    content_hash: target.content_hash.clone(),
                },
                ctx.now,
                ctx.actor.clone(),
            )?;
            self.validate_new_relation_endpoint(&relation)?;
            if self.relation_path(&relation.id).exists() {
                return Err(PulseError::validation(
                    "knowledge_relation_conflict",
                    format!("relation already exists: {}", relation.id),
                ));
            }
            relations.insert(relation.id.clone(), relation.clone());
            new_relations.push(relation);
        }
        let relation_ids = new_relations
            .iter()
            .map(|r| r.id.clone())
            .collect::<Vec<_>>();
        let learning = draft.into_learning(id.clone(), relation_ids.clone(), ctx.now);
        validate_learning_for_mutation(&self.repo_root, &learning, &relations)?;

        let entry_bytes = to_canonical_bytes(&learning)?;
        let mut targets = vec![TransactionTarget::new(
            self.entry_path(&id),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&entry_bytes),
                revision: 1,
            },
            &entry_bytes,
        )];
        for relation in &new_relations {
            let bytes = to_canonical_bytes(relation)?;
            targets.push(TransactionTarget::new(
                self.relation_path(&relation.id),
                FileState::Absent,
                FileState::Present {
                    hash: hash_bytes(&bytes),
                    revision: 1,
                },
                &bytes,
            ));
        }
        let event = EventEnvelope::new(
            new_event_id(),
            "knowledge.learning.created",
            ctx.actor.clone(),
            &id,
            json!({
                "learning_id": id,
                "revision_after": 1,
                "hash_after": hash_bytes(&entry_bytes),
                "relations": relation_ids,
            }),
            ctx.now,
        );
        let intent = MultiTargetTransactionIntent::prepared(
            event.id.clone(),
            "knowledge.learning.created",
            ctx.actor,
            targets,
            event_path(&self.repo_root, &event),
            serde_json::to_value(event)?,
        )?;
        let prepared = prepare_multi_target_transaction(&self.repo_root, intent)?;
        commit_prepared_multi_target_transaction(&prepared, self.failpoint)?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "created".to_string(),
            status: MutationStatus::Created,
            knowledge_fingerprint: knowledge_fingerprint(&self.repo_root, &manifest)?,
            value: learning,
            relations: relation_ids,
        })
    }

    pub fn show(&self, id: &str) -> PulseResult<LearningShow> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        bootstrap_unlocked(&self.repo_root)?;
        recover_prepared_transactions(&self.repo_root)?;
        let (entries, relations) = load_records(&self.repo_root)?;
        let learning = entries.get(id).cloned().ok_or_else(|| {
            PulseError::validation("learning_not_found", format!("learning not found: {id}"))
        })?;
        let mut rels: Vec<_> = relations
            .values()
            .filter(|r| r.from.id == id || (r.to.kind == EndpointKind::Learning && r.to.id == id))
            .cloned()
            .collect();
        rels.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(LearningShow {
            schema_version: 1,
            code: "ok".to_string(),
            learning,
            relations: rels,
        })
    }

    pub fn list(
        &self,
        status: Option<LearningStatus>,
        kind: Option<LearningKind>,
    ) -> PulseResult<ListOutcome<Learning>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        bootstrap_unlocked(&self.repo_root)?;
        recover_prepared_transactions(&self.repo_root)?;
        let (entries, _) = load_records(&self.repo_root)?;
        let mut items: Vec<_> = entries.into_values().collect();
        if let Some(status) = status {
            items.retain(|e| e.status == status);
        }
        if let Some(kind) = kind {
            items.retain(|e| e.kind == kind);
        }
        items.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(ListOutcome {
            schema_version: 1,
            code: "ok".to_string(),
            items,
        })
    }

    pub fn edit(
        &self,
        id: &str,
        expected_revision: u64,
        patch: LearningPatch,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Learning>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        let manifest = bootstrap_unlocked(&self.repo_root)?.manifest;
        recover_prepared_transactions(&self.repo_root)?;
        let (_, relations) = load_records(&self.repo_root)?;
        let path = self.entry_path(id);
        if !path.exists() {
            return Err(PulseError::validation(
                "learning_not_found",
                format!("learning not found: {id}"),
            ));
        }
        let before_bytes = fs::read(&path).map_err(|e| PulseError::io(&path, e))?;
        let before_hash = hash_bytes(&before_bytes);
        let mut learning: Learning =
            serde_json::from_slice(&before_bytes).map_err(|e| PulseError::json(&path, e))?;
        if learning.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: id.to_string(),
                expected_revision,
                current_revision: learning.revision,
            });
        }
        let changed = apply_patch(&mut learning, patch, ctx.now);
        if changed.is_empty() {
            return Ok(MutationOutcome {
                schema_version: 1,
                code: "unchanged".to_string(),
                status: MutationStatus::Unchanged,
                knowledge_fingerprint: knowledge_fingerprint(&self.repo_root, &manifest)?,
                value: learning,
                relations: Vec::new(),
            });
        }
        learning.revision += 1;
        learning.updated_at = ctx.now;
        learning.normalize();
        validate_learning_for_mutation(&self.repo_root, &learning, &relations)?;
        let after_bytes = to_canonical_bytes(&learning)?;
        self.commit_single("knowledge.learning.updated", ctx.actor, id, json!({"learning_id": id, "changed_fields": changed, "revision_before": expected_revision, "revision_after": learning.revision, "hash_before": before_hash, "hash_after": hash_bytes(&after_bytes)}), &path, FileState::Present { hash: before_hash, revision: expected_revision }, FileState::Present { hash: hash_bytes(&after_bytes), revision: learning.revision }, &after_bytes, ctx.now)?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "updated".to_string(),
            status: MutationStatus::Updated,
            knowledge_fingerprint: knowledge_fingerprint(&self.repo_root, &manifest)?,
            value: learning,
            relations: Vec::new(),
        })
    }

    pub fn add_relation(
        &self,
        learning_id: &str,
        args: RelationAdd,
        ctx: OperationContext,
    ) -> PulseResult<RelationOutcome> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        let manifest = bootstrap_unlocked(&self.repo_root)?.manifest;
        recover_prepared_transactions(&self.repo_root)?;
        let (mut entries, _relations) = load_records(&self.repo_root)?;
        let mut learning = entries.remove(learning_id).ok_or_else(|| {
            PulseError::validation(
                "learning_not_found",
                format!("learning not found: {learning_id}"),
            )
        })?;
        let relation = KnowledgeRelation::new(
            args.relation_type,
            learning_id.to_string(),
            Endpoint {
                kind: args.to_kind,
                id: args.to,
                revision: args.target_revision,
                content_hash: args.target_hash,
            },
            ctx.now,
            ctx.actor.clone(),
        )?;
        self.validate_new_relation_endpoint(&relation)?;
        let relation_path = self.relation_path(&relation.id);
        let relation_bytes = to_canonical_bytes(&relation)?;
        if relation_path.exists() {
            let existing: KnowledgeRelation = crate::storage::read_json(&relation_path)?;
            if existing.relation_type == relation.relation_type
                && existing.from == relation.from
                && existing.to == relation.to
            {
                return Ok(RelationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    relation_id: relation.id,
                    knowledge_fingerprint: Some(knowledge_fingerprint(&self.repo_root, &manifest)?),
                });
            }
            return Err(PulseError::validation(
                "knowledge_relation_conflict",
                format!(
                    "relation id conflicts with different payload: {}",
                    relation.id
                ),
            ));
        }
        validate_public_learning_claims(&learning)?;
        if learning.revision != args.expected_revision {
            return Err(PulseError::CasConflict {
                subject: learning_id.to_string(),
                expected_revision: args.expected_revision,
                current_revision: learning.revision,
            });
        }
        let before_entry_bytes = fs::read(self.entry_path(learning_id))
            .map_err(|e| PulseError::io(self.entry_path(learning_id), e))?;
        let before_hash = hash_bytes(&before_entry_bytes);
        let mut targets = Vec::new();
        let mut changed_entry = false;
        if relation.relation_type == RelationType::DerivedFrom {
            learning.provenance.relation_ids.push(relation.id.clone());
            changed_entry = true;
        } else if relation.relation_type == RelationType::PromotedTo {
            learning.promotion.relation_ids.push(relation.id.clone());
            changed_entry = true;
        }
        if changed_entry {
            learning.revision += 1;
            learning.updated_at = ctx.now;
            learning.normalize();
            let after_entry_bytes = to_canonical_bytes(&learning)?;
            targets.push(TransactionTarget::new(
                self.entry_path(learning_id),
                FileState::Present {
                    hash: before_hash.clone(),
                    revision: args.expected_revision,
                },
                FileState::Present {
                    hash: hash_bytes(&after_entry_bytes),
                    revision: learning.revision,
                },
                &after_entry_bytes,
            ));
        }
        targets.push(TransactionTarget::new(
            relation_path.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&relation_bytes),
                revision: 1,
            },
            &relation_bytes,
        ));
        let event = EventEnvelope::new(
            new_event_id(),
            "knowledge.relation.added",
            ctx.actor.clone(),
            learning_id,
            json!({"learning_id": learning_id, "relation_id": relation.id, "relation_type": relation.relation_type, "target_kind": relation.to.kind, "target_id": relation.to.id, "entry_revision_after": learning.revision}),
            ctx.now,
        );
        let intent = MultiTargetTransactionIntent::prepared(
            event.id.clone(),
            "knowledge.relation.added",
            ctx.actor,
            targets,
            event_path(&self.repo_root, &event),
            serde_json::to_value(event)?,
        )?;
        let prepared = prepare_multi_target_transaction(&self.repo_root, intent)?;
        commit_prepared_multi_target_transaction(&prepared, self.failpoint)?;
        Ok(RelationOutcome {
            schema_version: 1,
            code: "created".to_string(),
            status: MutationStatus::Created,
            relation_id: relation.id,
            knowledge_fingerprint: Some(knowledge_fingerprint(&self.repo_root, &manifest)?),
        })
    }

    pub fn validate(&self) -> PulseResult<crate::knowledge::validate::KnowledgeValidationReport> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        bootstrap_unlocked(&self.repo_root)?;
        recover_prepared_transactions(&self.repo_root)?;
        validate_knowledge(&self.repo_root)
    }

    pub fn export(&self) -> PulseResult<KnowledgeSnapshot> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        let manifest = bootstrap_unlocked(&self.repo_root)?.manifest;
        recover_prepared_transactions(&self.repo_root)?;
        let (entries, relations) = load_records(&self.repo_root)?;
        validate_loaded(&self.repo_root, &manifest, &entries, &relations)?.into_result()?;
        let snapshot = build_snapshot(
            &self.repo_root,
            &manifest,
            entries.into_values().collect(),
            relations.into_values().collect(),
        )?;
        write_snapshot_cache(&self.repo_root, &snapshot)?;
        Ok(snapshot)
    }

    pub fn status(&self) -> PulseResult<KnowledgeStatusReport> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        let manifest = bootstrap_unlocked(&self.repo_root)?.manifest;
        recover_prepared_transactions(&self.repo_root)?;
        let (entries, relations) = load_records(&self.repo_root)?;
        let report = validate_loaded(&self.repo_root, &manifest, &entries, &relations)?;
        let fingerprint = knowledge_fingerprint(&self.repo_root, &manifest).ok();
        let state = fingerprint
            .as_deref()
            .map(|f| cache_state(&self.repo_root, f))
            .unwrap_or(CacheState::Missing);
        Ok(KnowledgeStatusReport {
            schema_version: 1,
            code: "ok".to_string(),
            manifest: "current".to_string(),
            knowledge_fingerprint: fingerprint,
            counts: counts(
                &entries.into_values().collect::<Vec<_>>(),
                &relations.into_values().collect::<Vec<_>>(),
            ),
            cache_state: state,
            errors: report.errors.len(),
            warnings: report.warnings.len(),
        })
    }

    fn allocate_id(&self) -> PulseResult<String> {
        let mut max = 0;
        let dir = self.repo_root.join(".pulse/knowledge/entries");
        if dir.exists() {
            for entry in fs::read_dir(&dir).map_err(|e| PulseError::io(&dir, e))? {
                let path = entry.map_err(|e| PulseError::io(&dir, e))?.path();
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Some(n) = parse_numeric(stem, "LRN") {
                        max = max.max(n);
                    }
                }
            }
        }
        Ok(format!("LRN-{:03}", max + 1))
    }

    fn validate_new_relation_endpoint(&self, relation: &KnowledgeRelation) -> PulseResult<()> {
        if let Some(hash) = &relation.to.content_hash {
            if !validate_sha256(hash) {
                return Err(PulseError::validation(
                    "knowledge_relation_endpoint_hash_invalid",
                    "target hash must be sha256:<64 hex>",
                ));
            }
        }
        match relation.to.kind {
            EndpointKind::Learning => {
                if !self.entry_path(&relation.to.id).exists() {
                    return Err(PulseError::validation(
                        "knowledge_relation_endpoint_missing",
                        "target learning missing",
                    ));
                }
            }
            EndpointKind::Work => {
                let node = self.load_target_node(&relation.to.id).map_err(|_| {
                    PulseError::validation(
                        "knowledge_relation_endpoint_missing",
                        "target work missing",
                    )
                })?;
                self.ensure_revision_match(relation.to.revision, node.revision)?;
            }
            EndpointKind::Decision => {
                let node = self.load_target_node(&relation.to.id).map_err(|_| {
                    PulseError::validation(
                        "knowledge_relation_endpoint_missing",
                        "target decision missing",
                    )
                })?;
                if node.kind != crate::id::WorkKind::Decision {
                    return Err(PulseError::validation(
                        "knowledge_relation_endpoint_missing",
                        "target is not a decision",
                    ));
                }
                self.ensure_revision_match(relation.to.revision, node.revision)?;
            }
            EndpointKind::Document => {
                let registry: crate::docs::model::DocsRegistryEnvelope =
                    crate::storage::read_json(&self.repo_root.join(".pulse/docs/registry.json"))
                        .map_err(|_| {
                            PulseError::validation(
                                "knowledge_relation_endpoint_missing",
                                "target document missing",
                            )
                        })?;
                let Some(doc) = registry
                    .documents
                    .iter()
                    .find(|doc| doc.id == relation.to.id)
                else {
                    return Err(PulseError::validation(
                        "knowledge_relation_endpoint_missing",
                        "target document missing",
                    ));
                };
                self.ensure_revision_match(relation.to.revision, doc.revision)?;
            }
            EndpointKind::Receipt => {
                let (_receipt, hash) =
                    crate::evidence::receipt::load_receipt(&self.repo_root, &relation.to.id)
                        .map_err(|_| {
                            PulseError::validation(
                                "knowledge_relation_endpoint_missing",
                                "target receipt missing",
                            )
                        })?;
                if let Some(target_hash) = &relation.to.content_hash {
                    if *target_hash != hash {
                        return Err(PulseError::validation(
                            "knowledge_relation_endpoint_hash_mismatch",
                            "target receipt hash mismatch",
                        ));
                    }
                }
            }
            EndpointKind::Commit => {
                crate::source::resolve_full_commit(&self.repo_root, &relation.to.id).map_err(
                    |_| {
                        PulseError::validation(
                            "knowledge_relation_endpoint_missing",
                            "target commit missing",
                        )
                    },
                )?;
            }
        }
        Ok(())
    }

    fn load_target_node(&self, id: &str) -> PulseResult<crate::graph::node::Node> {
        let path = self
            .repo_root
            .join(".pulse/workgraph/nodes")
            .join(format!("{id}.json"));
        crate::storage::read_json(&path)
    }

    fn ensure_revision_match(&self, bound: Option<u64>, current: u64) -> PulseResult<()> {
        if let Some(bound) = bound {
            if bound != current {
                return Err(PulseError::validation(
                    "knowledge_relation_endpoint_revision_mismatch",
                    "target revision mismatch",
                ));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_single(
        &self,
        event_type: &str,
        actor: String,
        subject: &str,
        payload: serde_json::Value,
        target_path: &Path,
        before: FileState,
        after: FileState,
        canonical_bytes: &[u8],
        now: DateTime<Utc>,
    ) -> PulseResult<()> {
        let event = EventEnvelope::new(
            new_event_id(),
            event_type,
            actor.clone(),
            subject,
            payload,
            now,
        );
        let intent = TransactionIntent::prepared(
            event.id.clone(),
            event_type,
            actor,
            target_path.to_path_buf(),
            event_path(&self.repo_root, &event),
            before,
            after,
            serde_json::to_value(event)?,
        )?;
        let prepared = prepare_transaction(&self.repo_root, intent)?;
        commit_prepared_transaction(&prepared, canonical_bytes, self.failpoint)
    }
    fn entry_path(&self, id: &str) -> PathBuf {
        self.repo_root
            .join(".pulse/knowledge/entries")
            .join(format!("{id}.json"))
    }
    fn relation_path(&self, id: &str) -> PathBuf {
        self.repo_root
            .join(".pulse/knowledge/relations")
            .join(format!("{id}.json"))
    }
}

fn apply_patch(learning: &mut Learning, patch: LearningPatch, now: DateTime<Utc>) -> Vec<String> {
    let mut changed = Vec::new();
    if let Some(v) = patch.title {
        let v = v.trim().to_string();
        if learning.title != v {
            learning.title = v;
            changed.push("title".to_string());
        }
    }
    if let Some(v) = patch.severity {
        if learning.severity != v {
            learning.severity = v;
            changed.push("severity".to_string());
        }
    }
    if let Some(v) = patch.summary {
        let v = v.trim().to_string();
        if learning.summary != v {
            learning.summary = v;
            changed.push("summary".to_string());
        }
    }
    if let Some(v) = patch.guidance {
        if learning.guidance != v {
            learning.guidance = v;
            changed.push("guidance".to_string());
        }
    }
    if let Some(v) = patch.applicability {
        if learning.applicability != v {
            learning.applicability = v;
            changed.push("applicability".to_string());
        }
    }
    if let Some(v) = patch.routing {
        if learning.routing != v {
            learning.routing = v;
            changed.push("routing".to_string());
        }
    }
    if let Some(v) = patch.promotion {
        if learning.promotion != v {
            learning.promotion = v;
            changed.push("promotion".to_string());
        }
    }
    if let Some(v) = patch.freshness {
        if learning.freshness != v {
            learning.freshness = v;
            changed.push("freshness".to_string());
        }
    }
    if let Some(v) = patch.trust {
        if learning.trust != v {
            learning.trust = v;
            changed.push("trust".to_string());
        }
    }
    if let Some(v) = patch.content {
        if learning.content != v {
            learning.content = v;
            changed.push("content".to_string());
        }
    }
    if !changed.is_empty() {
        learning.updated_at = now;
    }
    changed
}
