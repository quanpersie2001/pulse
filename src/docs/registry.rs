use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::docs::manifest::{bootstrap_unlocked, load_existing_registry};
use crate::docs::model::{DocsRegistryEnvelope, DocumentLifecycle, DocumentPatch, DocumentRecord};
use crate::docs::validate::validate_registry;
use crate::event::{event_path, EventEnvelope};
use crate::id::new_event_id;
use crate::storage::transaction::{
    commit_prepared_transaction, current_file_state, prepare_transaction,
    recover_prepared_transactions, FileState, TransactionFailpoint, TransactionIntent,
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
    pub registry_revision: u64,
    pub value: T,
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

pub struct DocsRegistryStore {
    repo_root: PathBuf,
    failpoint: Option<TransactionFailpoint>,
}

impl DocsRegistryStore {
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

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn bootstrap(&self) -> PulseResult<DocsRegistryEnvelope> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        Ok(bootstrap_unlocked(&self.repo_root)?.registry)
    }

    pub fn load(&self) -> PulseResult<DocsRegistryEnvelope> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        crate::docs::manifest::load_unlocked(&self.repo_root)
    }

    pub fn list(&self) -> PulseResult<Vec<DocumentRecord>> {
        let registry = self.load()?;
        Ok(registry.documents)
    }

    pub fn show(&self, id: &str) -> PulseResult<DocumentRecord> {
        self.load()?
            .documents
            .into_iter()
            .find(|document| document.id == id)
            .ok_or_else(|| PulseError::NotFound {
                subject: format!("document {id}"),
            })
    }

    pub fn register(
        &self,
        expected_registry_revision: u64,
        mut document: DocumentRecord,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<DocumentRecord>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        let repository_id = bootstrap_unlocked(&self.repo_root)?.registry.repository_id;
        recover_prepared_transactions(&self.repo_root)?;
        let mut registry = load_existing_registry(&self.repo_root, &repository_id)?;
        expect_registry_revision(&registry, expected_registry_revision)?;
        if registry
            .documents
            .iter()
            .any(|existing| existing.id == document.id)
        {
            return Err(PulseError::AlreadyExists {
                subject: format!("document {}", document.id),
            });
        }

        let before_registry = registry.clone();
        document.revision = 1;
        document.normalize();
        registry.documents.push(document.clone());
        registry.revision += 1;
        registry.normalize();
        validate_registry(&self.repo_root, &repository_id, &registry)?.into_result()?;
        self.commit_registry_mutation(
            "docs.document.registered",
            ctx.actor,
            &document.id,
            json!({
                "document_id": document.id,
                "document_revision": document.revision,
                "registry_revision_before": before_registry.revision,
                "registry_revision_after": registry.revision,
                "registry_hash_before": registry_hash(&before_registry)?,
                "registry_hash_after": registry_hash(&registry)?,
            }),
            &before_registry,
            &registry,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "registered".to_string(),
            status: MutationStatus::Created,
            registry_revision: registry.revision,
            value: document,
        })
    }

    pub fn edit(
        &self,
        id: &str,
        expected_registry_revision: u64,
        expected_document_revision: u64,
        patch: DocumentPatch,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<DocumentRecord>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        let repository_id = bootstrap_unlocked(&self.repo_root)?.registry.repository_id;
        recover_prepared_transactions(&self.repo_root)?;
        let mut registry = load_existing_registry(&self.repo_root, &repository_id)?;
        expect_registry_revision(&registry, expected_registry_revision)?;
        let index = document_index(&registry, id)?;
        expect_document_revision(&registry.documents[index], expected_document_revision)?;

        let before_registry = registry.clone();
        let changed_fields = apply_patch(&mut registry.documents[index], patch);
        if changed_fields.is_empty() {
            return Ok(MutationOutcome {
                schema_version: 1,
                code: "unchanged".to_string(),
                status: MutationStatus::Unchanged,
                registry_revision: registry.revision,
                value: registry.documents[index].clone(),
            });
        }
        registry.documents[index].revision += 1;
        registry.revision += 1;
        registry.documents[index].normalize();
        registry.normalize();
        validate_registry(&self.repo_root, &repository_id, &registry)?.into_result()?;
        let document = registry
            .documents
            .iter()
            .find(|document| document.id == id)
            .cloned()
            .expect("edited document remains present");
        self.commit_registry_mutation(
            "docs.document.updated",
            ctx.actor,
            id,
            json!({
                "document_id": id,
                "document_revision_before": expected_document_revision,
                "document_revision_after": document.revision,
                "changed_fields": changed_fields,
                "registry_revision_before": before_registry.revision,
                "registry_revision_after": registry.revision,
                "registry_hash_before": registry_hash(&before_registry)?,
                "registry_hash_after": registry_hash(&registry)?,
            }),
            &before_registry,
            &registry,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "updated".to_string(),
            status: MutationStatus::Updated,
            registry_revision: registry.revision,
            value: document,
        })
    }

    pub fn retire(
        &self,
        id: &str,
        expected_registry_revision: u64,
        expected_document_revision: u64,
        reason: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<DocumentRecord>> {
        if reason.trim().is_empty() {
            return Err(PulseError::validation(
                "document_lifecycle_invalid",
                "retire reason must be non-empty",
            ));
        }
        self.lifecycle_mutation(
            id,
            expected_registry_revision,
            expected_document_revision,
            DocumentLifecycle::Retired,
            None,
            reason,
            "docs.document.retired",
            "retired",
            ctx,
        )
    }

    pub fn supersede(
        &self,
        old_id: &str,
        replacement_id: &str,
        expected_registry_revision: u64,
        expected_document_revision: u64,
        reason: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<DocumentRecord>> {
        if reason.trim().is_empty() {
            return Err(PulseError::validation(
                "document_lifecycle_invalid",
                "supersede reason must be non-empty",
            ));
        }
        if old_id == replacement_id {
            return Err(PulseError::validation(
                "document_supersession_cycle",
                "document cannot supersede itself",
            ));
        }
        self.lifecycle_mutation(
            old_id,
            expected_registry_revision,
            expected_document_revision,
            DocumentLifecycle::Superseded,
            Some(replacement_id.to_string()),
            reason,
            "docs.document.superseded",
            "superseded",
            ctx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn lifecycle_mutation(
        &self,
        id: &str,
        expected_registry_revision: u64,
        expected_document_revision: u64,
        lifecycle: DocumentLifecycle,
        superseded_by: Option<String>,
        reason: String,
        event_type: &str,
        code: &str,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<DocumentRecord>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        let repository_id = bootstrap_unlocked(&self.repo_root)?.registry.repository_id;
        recover_prepared_transactions(&self.repo_root)?;
        let mut registry = load_existing_registry(&self.repo_root, &repository_id)?;
        expect_registry_revision(&registry, expected_registry_revision)?;
        if let Some(target) = &superseded_by {
            let _ = document_index(&registry, target)?;
        }
        let index = document_index(&registry, id)?;
        expect_document_revision(&registry.documents[index], expected_document_revision)?;
        let before_registry = registry.clone();
        registry.documents[index].lifecycle = lifecycle;
        registry.documents[index].superseded_by = superseded_by.clone();
        registry.documents[index].revision += 1;
        registry.revision += 1;
        registry.normalize();
        validate_registry(&self.repo_root, &repository_id, &registry)?.into_result()?;
        let document = registry
            .documents
            .iter()
            .find(|document| document.id == id)
            .cloned()
            .expect("mutated document remains present");
        self.commit_registry_mutation(
            event_type,
            ctx.actor,
            id,
            json!({
                "document_id": id,
                "document_revision_before": expected_document_revision,
                "document_revision_after": document.revision,
                "reason": reason,
                "superseded_by": superseded_by,
                "registry_revision_before": before_registry.revision,
                "registry_revision_after": registry.revision,
                "registry_hash_before": registry_hash(&before_registry)?,
                "registry_hash_after": registry_hash(&registry)?,
            }),
            &before_registry,
            &registry,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: code.to_string(),
            status: MutationStatus::Updated,
            registry_revision: registry.revision,
            value: document,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_registry_mutation(
        &self,
        event_type: &str,
        actor: String,
        subject: &str,
        payload: serde_json::Value,
        before_registry: &DocsRegistryEnvelope,
        after_registry: &DocsRegistryEnvelope,
        now: DateTime<Utc>,
    ) -> PulseResult<()> {
        let target_path = self.registry_path();
        let before_bytes = to_canonical_bytes(before_registry)?;
        let after_bytes = to_canonical_bytes(after_registry)?;
        let before = FileState::Present {
            hash: hash_bytes(&before_bytes),
            revision: before_registry.revision,
        };
        let after = FileState::Present {
            hash: hash_bytes(&after_bytes),
            revision: after_registry.revision,
        };
        debug_assert_eq!(
            current_file_state(&target_path, Some(before_registry.revision))?,
            before
        );
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
            target_path,
            event_path(&self.repo_root, &event),
            before,
            after,
            serde_json::to_value(event)?,
        )?;
        let prepared = prepare_transaction(&self.repo_root, intent)?;
        commit_prepared_transaction(&prepared, &after_bytes, self.failpoint)
    }

    fn registry_path(&self) -> PathBuf {
        self.repo_root.join(".pulse/docs/registry.json")
    }
}

pub fn load_registry(repo_root: &Path) -> PulseResult<DocsRegistryEnvelope> {
    DocsRegistryStore::new(repo_root).load()
}

pub fn load_registry_unvalidated(repo_root: &Path) -> PulseResult<DocsRegistryEnvelope> {
    let _guard = WriteGuard::acquire(repo_root)?;
    recover_prepared_transactions(repo_root)?;
    let path = registry_path(repo_root);
    if !path.exists() {
        return Ok(bootstrap_unlocked(repo_root)?.registry);
    }
    crate::storage::read_json(&path)
}

pub fn load_registry_or_empty(repo_root: &Path) -> PulseResult<DocsRegistryEnvelope> {
    match crate::docs::manifest::load(repo_root) {
        Ok(registry) => Ok(registry),
        Err(error)
            if error.code() == "docs_registry_schema_invalid" || error.code() == "io_error" =>
        {
            let repository_id = crate::evidence::manifest::load(repo_root)?.repository_id;
            Ok(DocsRegistryEnvelope::empty(repository_id))
        }
        Err(error) => Err(error),
    }
}

pub fn registry_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".pulse/docs/registry.json")
}

pub fn registry_fingerprint(registry: &DocsRegistryEnvelope) -> PulseResult<String> {
    registry_hash(registry)
}

pub fn list(repo_root: &Path) -> PulseResult<Vec<DocumentRecord>> {
    DocsRegistryStore::new(repo_root).list()
}

pub fn show(repo_root: &Path, id: &str) -> PulseResult<DocumentRecord> {
    DocsRegistryStore::new(repo_root).show(id)
}

pub fn register(
    repo_root: &Path,
    expected_registry_revision: u64,
    document: DocumentRecord,
    actor: impl Into<String>,
) -> PulseResult<MutationOutcome<DocumentRecord>> {
    DocsRegistryStore::new(repo_root).register(
        expected_registry_revision,
        document,
        OperationContext {
            actor: actor.into(),
            now: Utc::now(),
        },
    )
}

pub fn edit(
    repo_root: &Path,
    id: &str,
    expected_registry_revision: u64,
    expected_document_revision: u64,
    patch: DocumentPatch,
    actor: impl Into<String>,
) -> PulseResult<MutationOutcome<DocumentRecord>> {
    DocsRegistryStore::new(repo_root).edit(
        id,
        expected_registry_revision,
        expected_document_revision,
        patch,
        OperationContext {
            actor: actor.into(),
            now: Utc::now(),
        },
    )
}

pub fn retire(
    repo_root: &Path,
    id: &str,
    expected_registry_revision: u64,
    expected_document_revision: u64,
    reason: impl Into<String>,
    actor: impl Into<String>,
) -> PulseResult<MutationOutcome<DocumentRecord>> {
    DocsRegistryStore::new(repo_root).retire(
        id,
        expected_registry_revision,
        expected_document_revision,
        reason.into(),
        OperationContext {
            actor: actor.into(),
            now: Utc::now(),
        },
    )
}

pub fn supersede(
    repo_root: &Path,
    old_id: &str,
    replacement_id: &str,
    expected_registry_revision: u64,
    expected_document_revision: u64,
    reason: impl Into<String>,
    actor: impl Into<String>,
) -> PulseResult<MutationOutcome<DocumentRecord>> {
    DocsRegistryStore::new(repo_root).supersede(
        old_id,
        replacement_id,
        expected_registry_revision,
        expected_document_revision,
        reason.into(),
        OperationContext {
            actor: actor.into(),
            now: Utc::now(),
        },
    )
}

fn expect_registry_revision(
    registry: &DocsRegistryEnvelope,
    expected_registry_revision: u64,
) -> PulseResult<()> {
    if registry.revision != expected_registry_revision {
        return Err(PulseError::CasConflict {
            subject: "docs registry".to_string(),
            expected_revision: expected_registry_revision,
            current_revision: registry.revision,
        });
    }
    Ok(())
}

fn expect_document_revision(
    document: &DocumentRecord,
    expected_document_revision: u64,
) -> PulseResult<()> {
    if document.revision != expected_document_revision {
        return Err(PulseError::CasConflict {
            subject: format!("document {}", document.id),
            expected_revision: expected_document_revision,
            current_revision: document.revision,
        });
    }
    Ok(())
}

fn document_index(registry: &DocsRegistryEnvelope, id: &str) -> PulseResult<usize> {
    registry
        .documents
        .iter()
        .position(|document| document.id == id)
        .ok_or_else(|| PulseError::NotFound {
            subject: format!("document {id}"),
        })
}

fn apply_patch(document: &mut DocumentRecord, patch: DocumentPatch) -> Vec<String> {
    let mut changed = BTreeSet::new();
    if let Some(value) = patch.path {
        if document.path != value {
            document.path = value;
            changed.insert("path".to_string());
        }
    }
    if let Some(value) = patch.owner {
        if document.owner != value {
            document.owner = value;
            changed.insert("owner".to_string());
        }
    }
    if let Some(value) = patch.summary {
        if document.summary != value {
            document.summary = value;
            changed.insert("summary".to_string());
        }
    }
    if let Some(value) = patch.aliases {
        if document.aliases != value {
            document.aliases = value;
            changed.insert("aliases".to_string());
        }
    }
    if let Some(value) = patch.scope {
        if document.scope != value {
            document.scope = value;
            changed.insert("scope".to_string());
        }
    }
    if let Some(value) = patch.authority {
        if document.authority != value {
            document.authority = value;
            changed.insert("authority".to_string());
        }
    }
    if let Some(value) = patch.lifecycle {
        if document.lifecycle != value {
            document.lifecycle = value;
            changed.insert("lifecycle".to_string());
        }
    }
    if let Some(value) = patch.review_policy {
        if document.review_policy != value {
            document.review_policy = value;
            changed.insert("review_policy".to_string());
        }
    }
    if let Some(value) = patch.verification_profile {
        if document.verification_profile != value {
            document.verification_profile = value;
            changed.insert("verification_profile".to_string());
        }
    }
    if let Some(value) = patch.generated {
        if document.generated != value {
            document.generated = value;
            changed.insert("generated".to_string());
        }
    }
    if let Some(value) = patch.superseded_by {
        if document.superseded_by != value {
            document.superseded_by = value;
            changed.insert("superseded_by".to_string());
        }
    }
    changed.into_iter().collect()
}

fn registry_hash(registry: &DocsRegistryEnvelope) -> PulseResult<String> {
    Ok(hash_bytes(&to_canonical_bytes(registry)?))
}
