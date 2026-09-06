use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::new_event_id;
use crate::event::{event_path, EventEnvelope};
use crate::id::parse_numeric;
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
    /// Aggregated handoff-reported usage feedback for this learning.
    pub usage: KnowledgeUsageSummary,
}

/// Usage feedback aggregated over every handoff receipt in the repository.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeUsageSummary {
    pub helpful: u64,
    pub not_needed: u64,
    pub misleading: u64,
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

    pub fn repo_root(&self) -> &std::path::Path {
        &self.repo_root
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
        // Learning text is tracked-plane content (Decision 0012 §4): rewrite
        // in-repo absolute paths to repository-relative, refuse secrets.
        let mut draft = draft;
        draft.title =
            crate::evidence::redaction::clean_text(&self.repo_root, "title", &draft.title)?;
        draft.summary =
            crate::evidence::redaction::clean_text(&self.repo_root, "summary", &draft.summary)?;
        for field in [
            ("guidance.do", &mut draft.guidance.r#do),
            ("guidance.avoid", &mut draft.guidance.avoid),
            (
                "guidance.required_checks",
                &mut draft.guidance.required_checks,
            ),
        ] {
            for line in field.1.iter_mut() {
                *line = crate::evidence::redaction::clean_text(&self.repo_root, field.0, line)?;
            }
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
        let usage = learning_usage_counts(&self.repo_root, id)?;
        Ok(LearningShow {
            schema_version: 1,
            code: "ok".to_string(),
            learning,
            relations: rels,
            usage,
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

    /// Ratchet lifecycle transition.
    ///
    /// `candidate|reviewed -> validated` requires an evidence receipt id
    /// that resolves in the evidence store. `validated -> promoted`
    /// requires a registry document and records a `promoted_to` relation
    /// bound to the document's current revision and content hash in the
    /// same transaction.
    pub fn transition_status(
        &self,
        id: &str,
        to: LearningStatus,
        evidence_receipt: Option<&str>,
        document_id: Option<&str>,
        rationale: Option<String>,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Learning>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        let manifest = bootstrap_unlocked(&self.repo_root)?.manifest;
        recover_prepared_transactions(&self.repo_root)?;
        let (_entries, mut relations) = load_records(&self.repo_root)?;
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
        let from = learning.status;
        let mut new_relations: Vec<KnowledgeRelation> = Vec::new();
        match (from, to) {
            (LearningStatus::Candidate | LearningStatus::Reviewed, LearningStatus::Validated) => {
                let receipt_id = evidence_receipt.ok_or_else(|| {
                    PulseError::validation(
                        "knowledge_validate_evidence_missing",
                        "validating a learning requires --evidence <receipt-id>",
                    )
                })?;
                crate::evidence::receipt::load_receipt(&self.repo_root, receipt_id).map_err(
                    |error| {
                        PulseError::validation(
                            "knowledge_validate_evidence_missing",
                            format!("evidence receipt does not resolve: {error}"),
                        )
                    },
                )?;
                learning.validation.validated_by.push(ctx.actor.clone());
                learning.validation.validated_at = Some(ctx.now);
                if learning.validation.confidence == Confidence::Low {
                    learning.validation.confidence = Confidence::Medium;
                }
            }
            (LearningStatus::Validated, LearningStatus::Promoted) => {
                let doc_id = document_id.ok_or_else(|| {
                    PulseError::validation(
                        "knowledge_promote_document_missing",
                        "promoting a learning requires --document <doc-id>",
                    )
                })?;
                let registry: crate::docs::model::DocsRegistryEnvelope =
                    crate::storage::read_json(&self.repo_root.join(".pulse/docs/registry.json"))
                        .map_err(|_| {
                            PulseError::validation(
                                "knowledge_relation_endpoint_missing",
                                "target document missing",
                            )
                        })?;
                let doc = registry
                    .documents
                    .iter()
                    .find(|doc| doc.id == doc_id)
                    .ok_or_else(|| {
                        PulseError::validation(
                            "knowledge_relation_endpoint_missing",
                            format!("target document missing: {doc_id}"),
                        )
                    })?;
                let doc_bytes = fs::read(self.repo_root.join(&doc.path))
                    .map_err(|error| PulseError::io(self.repo_root.join(&doc.path), error))?;
                let relation = KnowledgeRelation::new(
                    RelationType::PromotedTo,
                    id.to_string(),
                    Endpoint {
                        kind: EndpointKind::Document,
                        id: doc.id.clone(),
                        revision: Some(doc.revision),
                        content_hash: Some(hash_bytes(&doc_bytes)),
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
                learning.promotion.state = PromotionState::Promoted;
                learning.promotion.rationale = rationale;
                relations.insert(relation.id.clone(), relation.clone());
                new_relations.push(relation);
            }
            _ => {
                return Err(PulseError::validation(
                    "knowledge_transition_invalid",
                    format!("cannot transition learning from {from:?} to {to:?}"),
                ))
            }
        }
        learning.status = to;
        learning.revision += 1;
        learning.updated_at = ctx.now;
        learning.normalize();
        learning.promotion.relation_ids = learning
            .promotion
            .relation_ids
            .iter()
            .cloned()
            .chain(new_relations.iter().map(|r| r.id.clone()))
            .collect();
        crate::knowledge::validate::validate_learning_for_transition(
            &self.repo_root,
            &learning,
            &relations,
        )?;
        let after_bytes = to_canonical_bytes(&learning)?;
        let mut targets = vec![TransactionTarget::new(
            self.entry_path(id),
            FileState::Present {
                hash: before_hash,
                revision: learning.revision - 1,
            },
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: learning.revision,
            },
            &after_bytes,
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
        let relation_ids = new_relations
            .iter()
            .map(|r| r.id.clone())
            .collect::<Vec<_>>();
        let event = EventEnvelope::new(
            new_event_id(),
            "knowledge.learning.transitioned",
            ctx.actor.clone(),
            id,
            json!({
                "learning_id": id,
                "from": from,
                "to": to,
                "revision_after": learning.revision,
                "hash_after": hash_bytes(&after_bytes),
                "relations": relation_ids,
            }),
            ctx.now,
        );
        let intent = MultiTargetTransactionIntent::prepared(
            event.id.clone(),
            "knowledge.learning.transitioned",
            ctx.actor,
            targets,
            event_path(&self.repo_root, &event),
            serde_json::to_value(event)?,
        )?;
        let prepared = prepare_multi_target_transaction(&self.repo_root, intent)?;
        commit_prepared_multi_target_transaction(&prepared, self.failpoint)?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "transitioned".to_string(),
            status: MutationStatus::Updated,
            knowledge_fingerprint: knowledge_fingerprint(&self.repo_root, &manifest)?,
            value: learning,
            relations: relation_ids,
        })
    }

    /// Promote a validated learning by inserting its content into a target
    /// document (Decision 13.4): the text block built from summary and
    /// guidance is inserted after the given heading, the document is written,
    /// and only then is the `promoted_to` relation recorded bound to the
    /// document's NEW content hash. A promotion that would leave the target
    /// byte-identical (e.g. the block is already present) fails with
    /// `promotion_target_unchanged`. `dry_run` reports the proposed insertion
    /// without writing or transitioning.
    pub fn promote_learning(
        &self,
        args: PromoteArgs<'_>,
        ctx: OperationContext,
    ) -> PulseResult<PromoteOutcome> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        let _manifest = bootstrap_unlocked(&self.repo_root)?.manifest;
        recover_prepared_transactions(&self.repo_root)?;
        let (_entries, mut relations) = load_records(&self.repo_root)?;
        let path = self.entry_path(args.learning_id);
        if !path.exists() {
            return Err(PulseError::validation(
                "learning_not_found",
                format!("learning not found: {}", args.learning_id),
            ));
        }
        let before_bytes = fs::read(&path).map_err(|e| PulseError::io(&path, e))?;
        let before_hash = hash_bytes(&before_bytes);
        let mut learning: Learning =
            serde_json::from_slice(&before_bytes).map_err(|e| PulseError::json(&path, e))?;
        // Re-promotion (Decision 13.4): a promoted learning can move to a new
        // target; its previous promoted_to relations are retired in the same
        // transaction so exactly one promotion target stays current.
        let from_status = learning.status;
        let mut retired_relation_ids: Vec<String> = Vec::new();
        if learning.status == LearningStatus::Promoted {
            retired_relation_ids = learning.promotion.relation_ids.clone();
        } else if learning.status != LearningStatus::Validated {
            return Err(PulseError::validation(
                "knowledge_transition_invalid",
                format!(
                    "only validated or promoted learnings can be promoted; {} is {:?}",
                    learning.id, learning.status
                ),
            ));
        }

        // Resolve the target file and relation endpoint identity.
        let (target_path, endpoint_id, endpoint_revision) = match &args.target {
            PromoteTarget::Document(doc_id) => {
                let registry: crate::docs::model::DocsRegistryEnvelope =
                    crate::storage::read_json(&self.repo_root.join(".pulse/docs/registry.json"))
                        .map_err(|_| {
                            PulseError::validation(
                                "knowledge_relation_endpoint_missing",
                                "target document missing",
                            )
                        })?;
                let doc = registry
                    .documents
                    .iter()
                    .find(|doc| &doc.id == doc_id)
                    .ok_or_else(|| {
                        PulseError::validation(
                            "knowledge_relation_endpoint_missing",
                            format!("target document missing: {doc_id}"),
                        )
                    })?;
                (
                    self.repo_root.join(&doc.path),
                    doc.id.clone(),
                    Some(doc.revision),
                )
            }
            PromoteTarget::AgentsMd => (
                self.repo_root.join("AGENTS.md"),
                "AGENTS.md".to_string(),
                None,
            ),
        };
        let target_bytes =
            fs::read(&target_path).map_err(|error| PulseError::io(&target_path, error))?;
        let target_text = String::from_utf8(target_bytes.clone()).map_err(|_| {
            PulseError::validation(
                "promotion_target_invalid",
                "promotion target must be UTF-8 markdown",
            )
        })?;

        let block = promotion_block(&learning);
        let heading_line = find_heading_line(&target_text, args.insert_after)?;
        // Insert below the heading and its blank-line separator so the block
        // opens the heading's section.
        let mut insert_at = heading_line + 1;
        let lines: Vec<&str> = target_text.lines().collect();
        while insert_at < lines.len() && lines[insert_at].trim().is_empty() {
            insert_at += 1;
        }
        if target_text.contains(&block) {
            return Err(PulseError::validation(
                "promotion_target_unchanged",
                format!(
                    "{} already contains the promotion block for {}; nothing to insert",
                    target_path.display(),
                    learning.id
                ),
            ));
        }
        let updated = insert_after_line(&target_text, insert_at.saturating_sub(1), &block);
        let line_of_block = target_text[..target_text
            .lines()
            .take(insert_at)
            .map(|line| line.len() + 1)
            .sum::<usize>()]
            .lines()
            .count() as u64
            + 1;
        if args.dry_run {
            return Ok(PromoteOutcome {
                schema_version: 1,
                code: "promotion_dry_run".to_string(),
                dry_run: true,
                learning_id: learning.id.clone(),
                target_path: target_path
                    .strip_prefix(&self.repo_root)
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_else(|_| target_path.display().to_string()),
                insert_after: args.insert_after.to_string(),
                inserted_at_line: line_of_block,
                inserted_block: block,
                target_content_hash: hash_bytes(updated.as_bytes()),
            });
        }

        // The promotion must change the target document (Decision 13.4).
        crate::storage::atomic_write(&target_path, updated.as_bytes())?;
        let after_bytes =
            fs::read(&target_path).map_err(|error| PulseError::io(&target_path, error))?;
        let hash_after = hash_bytes(&after_bytes);
        if hash_after == hash_bytes(&target_bytes) {
            return Err(PulseError::validation(
                "promotion_target_unchanged",
                format!(
                    "writing {} did not change its content; refusing to record a promotion",
                    target_path.display()
                ),
            ));
        }

        let relation = KnowledgeRelation::new(
            RelationType::PromotedTo,
            learning.id.clone(),
            Endpoint {
                kind: EndpointKind::Document,
                id: endpoint_id,
                revision: endpoint_revision,
                content_hash: Some(hash_after.clone()),
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
        learning.promotion.state = PromotionState::Promoted;
        learning.promotion.rationale = args.rationale;
        // Missing retired ids are tolerated: the promotion list is replaced
        // wholesale below either way.
        for retired in &retired_relation_ids {
            relations.remove(retired);
        }
        relations.insert(relation.id.clone(), relation.clone());
        learning.status = LearningStatus::Promoted;
        learning.revision += 1;
        learning.updated_at = ctx.now;
        learning.normalize();
        learning.promotion.relation_ids = vec![relation.id.clone()];
        crate::knowledge::validate::validate_learning_for_transition(
            &self.repo_root,
            &learning,
            &relations,
        )?;
        let learning_after = to_canonical_bytes(&learning)?;
        let relation_bytes = to_canonical_bytes(&relation)?;
        let mut targets = vec![TransactionTarget::new(
            path,
            FileState::Present {
                hash: before_hash,
                revision: learning.revision - 1,
            },
            FileState::Present {
                hash: hash_bytes(&learning_after),
                revision: learning.revision,
            },
            &learning_after,
        )];
        targets.push(TransactionTarget::new(
            self.relation_path(&relation.id),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&relation_bytes),
                revision: 1,
            },
            &relation_bytes,
        ));
        for retired in &retired_relation_ids {
            let retired_path = self.relation_path(retired);
            if !retired_path.exists() {
                continue;
            }
            let retired_bytes =
                fs::read(&retired_path).map_err(|error| PulseError::io(&retired_path, error))?;
            targets.push(TransactionTarget::new(
                retired_path,
                FileState::Present {
                    hash: hash_bytes(&retired_bytes),
                    revision: 1,
                },
                FileState::Absent,
                &[],
            ));
        }
        let event = EventEnvelope::new(
            new_event_id(),
            "knowledge.learning.transitioned",
            ctx.actor.clone(),
            &learning.id,
            json!({
                "learning_id": learning.id,
                "from": from_status,
                "to": "promoted",
                "revision_after": learning.revision,
                "hash_after": hash_bytes(&learning_after),
                "relations": [relation.id],
                "promotion_target_hash": hash_after,
            }),
            ctx.now,
        );
        let intent = MultiTargetTransactionIntent::prepared(
            event.id.clone(),
            "knowledge.learning.transitioned",
            ctx.actor,
            targets,
            event_path(&self.repo_root, &event),
            serde_json::to_value(event)?,
        )?;
        let prepared = prepare_multi_target_transaction(&self.repo_root, intent)?;
        commit_prepared_multi_target_transaction(&prepared, self.failpoint)?;
        Ok(PromoteOutcome {
            schema_version: 1,
            code: "promoted".to_string(),
            dry_run: false,
            learning_id: learning.id.clone(),
            target_path: target_path
                .strip_prefix(&self.repo_root)
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|_| target_path.display().to_string()),
            insert_after: args.insert_after.to_string(),
            inserted_at_line: line_of_block,
            inserted_block: block,
            target_content_hash: hash_after,
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
        if has_pending_transaction(&self.repo_root)? {
            return Err(PulseError::validation(
                "knowledge_recovery_required",
                "knowledge status requires prepared transaction recovery before observing a coherent snapshot",
            ));
        }
        let Some(manifest) = crate::knowledge::manifest::load_existing(&self.repo_root)? else {
            return Ok(KnowledgeStatusReport {
                schema_version: 1,
                code: "not_installed".to_string(),
                manifest: "not_installed".to_string(),
                knowledge_fingerprint: None,
                counts: crate::knowledge::projection::KnowledgeCounts {
                    entries: 0,
                    relations: 0,
                    by_status: Default::default(),
                    by_kind: Default::default(),
                },
                cache_state: CacheState::Missing,
                errors: 0,
                warnings: 0,
            });
        };
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
                let registered =
                    crate::storage::read_json::<crate::docs::model::DocsRegistryEnvelope>(
                        self.repo_root.join(".pulse/docs/registry.json").as_path(),
                    )
                    .ok()
                    .and_then(|registry| {
                        registry
                            .documents
                            .iter()
                            .find(|doc| doc.id == relation.to.id)
                            .map(|doc| doc.revision)
                    });
                match registered {
                    Some(doc_revision) => {
                        self.ensure_revision_match(relation.to.revision, doc_revision)?
                    }
                    // A document outside the registry (e.g. the repository
                    // map AGENTS.md) resolves as a repository file.
                    None => {
                        let path = self.repo_root.join(&relation.to.id);
                        if !path.is_file() {
                            return Err(PulseError::validation(
                                "knowledge_relation_endpoint_missing",
                                "target document missing",
                            ));
                        }
                    }
                }
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

    fn load_target_node(&self, id: &str) -> PulseResult<crate::graph::model::node::Node> {
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

fn has_pending_transaction(repo_root: &Path) -> PulseResult<bool> {
    let path = repo_root.join(".pulse/runtime/transactions");
    if !path.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(&path).map_err(|error| PulseError::io(&path, error))? {
        let entry = entry.map_err(|error| PulseError::io(&path, error))?;
        if entry
            .file_type()
            .map_err(|error| PulseError::io(entry.path(), error))?
            .is_file()
            && entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("json")
        {
            return Ok(true);
        }
    }
    Ok(false)
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
    if let Some(v) = patch.scope {
        if learning.scope != v {
            learning.scope = v;
            changed.push("scope".to_string());
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

/// Promotion target: a registry document or the repository map.
#[derive(Debug, Clone)]
pub enum PromoteTarget {
    Document(String),
    AgentsMd,
}

/// Arguments for [`KnowledgeStore::promote_learning`].
#[derive(Debug, Clone)]
pub struct PromoteArgs<'a> {
    pub learning_id: &'a str,
    pub target: PromoteTarget,
    /// Heading whose section the promotion block is inserted under.
    pub insert_after: &'a str,
    pub dry_run: bool,
    pub rationale: Option<String>,
}

/// Result of one `knowledge promote` invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PromoteOutcome {
    pub schema_version: u32,
    pub code: String,
    pub dry_run: bool,
    pub learning_id: String,
    pub target_path: String,
    pub insert_after: String,
    /// 1-based line where the inserted block starts.
    pub inserted_at_line: u64,
    pub inserted_block: String,
    /// Content hash of the target document after the insertion.
    pub target_content_hash: String,
}

/// The deterministic markdown block a promotion inserts: title, summary and
/// the guidance bullets of the learning.
fn promotion_block(learning: &Learning) -> String {
    let mut lines = vec![format!(
        "- **{} — {}**: {}",
        learning.id, learning.title, learning.summary
    )];
    for item in &learning.guidance.r#do {
        lines.push(format!("  - Do: {item}"));
    }
    for item in &learning.guidance.avoid {
        lines.push(format!("  - Avoid: {item}"));
    }
    for item in &learning.guidance.required_checks {
        lines.push(format!("  - Check: {item}"));
    }
    lines.join("\n")
}

/// Find the unique ATX heading matching `needle`: exact heading-text match
/// first, then unique substring containment. Errors name the ambiguity.
fn find_heading_line(text: &str, needle: &str) -> PulseResult<usize> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Err(PulseError::validation(
            "promotion_target_heading_missing",
            "insert-after heading must not be empty",
        ));
    }
    let headings = text
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let text = line.trim().strip_prefix('#')?.trim_start();
            Some((index, text.to_ascii_lowercase()))
        })
        .collect::<Vec<_>>();
    let exact: Vec<usize> = headings
        .iter()
        .filter(|(_, text)| text == &needle.to_ascii_lowercase())
        .map(|(index, _)| *index)
        .collect();
    let matches = if exact.len() == 1 {
        exact
    } else {
        let partial: Vec<usize> = headings
            .iter()
            .filter(|(_, text)| text.contains(&needle.to_ascii_lowercase()))
            .map(|(index, _)| *index)
            .collect();
        match partial.len() {
            0 => {
                return Err(PulseError::validation(
                    "promotion_target_heading_missing",
                    format!("no heading matches {needle:?}"),
                ))
            }
            1 => partial,
            _ => {
                return Err(PulseError::validation(
                    "promotion_target_heading_ambiguous",
                    format!(
                        "{partial_len} headings match {needle:?}; use a longer heading",
                        partial_len = partial.len()
                    ),
                ))
            }
        }
    };
    Ok(matches[0])
}

/// Insert `block` (plus one separating blank line) after 0-based `line`.
fn insert_after_line(text: &str, line: usize, block: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    let at = (line + 1).min(lines.len());
    let mut block_lines: Vec<&str> = block.lines().collect();
    block_lines.push("");
    lines.splice(at..at, block_lines);
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Aggregate handoff-reported usage feedback for one learning by scanning the
/// execution handoff receipts. Unknown or malformed receipts are skipped:
/// usage is advisory signal, not proof.
fn learning_usage_counts(
    repo_root: &Path,
    learning_id: &str,
) -> PulseResult<KnowledgeUsageSummary> {
    let directory = repo_root.join(".pulse/evidence/execution/handoffs");
    let mut summary = KnowledgeUsageSummary::default();
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(summary),
        Err(error) => return Err(PulseError::io(&directory, error)),
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(error) => return Err(PulseError::io(&directory, error)),
        };
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        let Some(usage) = value
            .get("knowledge_usage")
            .and_then(|usage| usage.as_array())
        else {
            continue;
        };
        for claim in usage {
            if claim.get("learning_id").and_then(|id| id.as_str()) != Some(learning_id) {
                continue;
            }
            match claim.get("outcome").and_then(|outcome| outcome.as_str()) {
                Some("helpful") => summary.helpful += 1,
                Some("not_needed") => summary.not_needed += 1,
                Some("misleading") => summary.misleading += 1,
                _ => {}
            }
        }
    }
    Ok(summary)
}
