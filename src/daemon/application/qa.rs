//! Structured QA checkpoint execution owned by the daemon application.
//!
//! This saga reads an allowlisted executor contract from the target repository,
//! commits exact execution intent before process I/O, delegates process-tree
//! ownership to [`ProcessOwner`], and records the resulting immutable Core
//! receipt. Unknown external outcomes fail closed and are never blindly rerun.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{append_event, deterministic_id, external_effect_blocked, DaemonApplication};
use crate::canonical_json::{hash_serializable, to_canonical_bytes};
use crate::daemon::assignment::AssignmentSagaState;
use crate::daemon::persistence::{ExternalEffectKind, ExternalEffectState};
use crate::daemon::process::RunRequest;
use crate::daemon::protocol::DaemonResponse;
use crate::evidence::model::{
    ArtifactBinding, ContentBinding, ReceiptBindings, ReceiptEnvelope, ReceiptKind, ReceiptPayload,
    ReceiptResult, SourceBinding, SubjectRef,
};
use crate::qa::{
    QaBaselineResolution, QaCaseObservation, QaCaseOutcome, QaCheckpointPayload, QaExecutionScope,
    QaExecutor, QaExecutorManifest, QaRunnerInput, QaRunnerOutput, QaRuntimeEnvironment,
};
use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QaRunPlan {
    schema_version: u32,
    receipt_id: String,
    recorded_at: chrono::DateTime<Utc>,
    executor_manifest: QaExecutorManifest,
    executor_path: String,
    executor_manifest_hash: String,
    input: QaRunnerInput,
}

impl DaemonApplication {
    pub(super) fn qa_checkpoint_run(
        &self,
        saga_id: &str,
        actor: &str,
        source_commit: &str,
        executor_id: &str,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        let saga = self.assignment_saga(saga_id)?;
        if saga.state != AssignmentSagaState::Verifying || saga.handoff_id.is_none() {
            return Err(PulseError::validation(
                "assignment_not_verifying",
                "QA checkpoint requires a submitted handoff",
            ));
        }
        if saga.verification_id.is_some() {
            return Err(PulseError::validation(
                "qa_checkpoint_too_late",
                "QA checkpoint must run before immutable verification is recorded",
            ));
        }
        let project = self.project_record(&saga.project_id)?;
        let workspace_id = saga.workspace_id.as_deref().ok_or_else(|| {
            PulseError::validation("assignment_saga_invalid", "saga has no workspace")
        })?;
        let workspace = self.store.with_state(false, |state| {
            state
                .workspaces
                .get(workspace_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("workspace {workspace_id}"),
                })
        })?;
        let repo_root = Path::new(&project.canonical_root);
        let workspace_root = PathBuf::from(&workspace.root);
        if crate::source::head_commit(repo_root)? != source_commit {
            return Err(PulseError::validation(
                "qa_source_stale",
                "QA source commit must equal the current canonical repository HEAD",
            ));
        }
        if crate::source::head_commit(&workspace_root)? != source_commit {
            return Err(PulseError::validation(
                "qa_workspace_source_stale",
                "QA executor workspace must be checked out at the requested source commit",
            ));
        }
        let core = crate::JsonGraphStore::new(repo_root);
        let ticket = core.show_node(&saga.ticket_id)?;
        let baseline = crate::qa::resolve_ticket_cases(repo_root, &ticket)?;
        let effect_id = deterministic_id("effect_qa", &format!("{saga_id}:{idempotency_key}"));
        let request_fingerprint = hash_serializable(&(
            saga_id,
            actor,
            source_commit,
            executor_id,
            &baseline.content_hash,
        ))?;
        let existing = self.store.with_state(false, |state| {
            Ok(state.external_effects.get(&effect_id).cloned())
        })?;
        let plan = match existing
            .as_ref()
            .and_then(|effect| effect.request_message.as_ref())
        {
            Some(message) => serde_json::from_str::<QaRunPlan>(message).map_err(|error| {
                PulseError::validation(
                    "qa_execution_plan_invalid",
                    format!("durable QA execution plan is invalid: {error}"),
                )
            })?,
            None => build_plan(
                &workspace_root,
                &saga.ticket_id,
                source_commit,
                executor_id,
                baseline.clone(),
                &effect_id,
            )?,
        };
        let plan_json = String::from_utf8(to_canonical_bytes(&plan)?).map_err(|_| {
            PulseError::validation("qa_execution_plan_invalid", "QA plan is not UTF-8")
        })?;
        let effect = self.record_external_effect(
            &effect_id,
            ExternalEffectKind::QaCheckpointRun,
            saga_id,
            &request_fingerprint,
            format!("run QA executor {executor_id}"),
            Some(plan_json),
        )?;
        match effect.state {
            ExternalEffectState::Acknowledged => {
                return self.recover_qa_receipt(saga_id, &effect_id, &plan, repo_root)
            }
            ExternalEffectState::Attempting | ExternalEffectState::OutcomeUnknown => {
                if crate::evidence::show_receipt(repo_root, &plan.receipt_id).is_ok() {
                    return self.recover_qa_receipt(saga_id, &effect_id, &plan, repo_root);
                }
                return Err(external_effect_blocked(&effect_id));
            }
            ExternalEffectState::DefinitivelyFailed => {
                return Err(external_effect_blocked(&effect_id));
            }
            ExternalEffectState::NotSent => {}
        }
        if crate::source::check_cleanliness(&workspace_root)?
            != crate::source::SourceCleanliness::Clean
        {
            return Err(PulseError::validation(
                "qa_workspace_source_dirty",
                "QA executor workspace must be clean before checkpoint execution",
            ));
        }

        let executable = resolve_tracked_executable(&workspace_root, &plan.executor_path)?;
        let input_path = self
            .store
            .root()
            .join("qa-inputs")
            .join(format!("{effect_id}.json"));
        if let Some(parent) = input_path.parent() {
            fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        }
        crate::storage::atomic_write_private(&input_path, &to_canonical_bytes(&plan.input)?)?;
        self.update_external_effect(
            &effect_id,
            ExternalEffectState::Attempting,
            None,
            Some("QA executor process started".to_string()),
        )?;
        let mut args = plan.executor_manifest.args.clone();
        args.push(input_path.to_string_lossy().to_string());
        let persist_started = |record: &crate::daemon::process::HelperProcessRecord| {
            self.store.with_state(true, |state| {
                let effect = state.external_effects.get_mut(&effect_id).ok_or_else(|| {
                    PulseError::NotFound {
                        subject: format!("external effect {effect_id}"),
                    }
                })?;
                effect.attempt_process = Some(record.clone());
                effect.updated_at = Utc::now().to_rfc3339();
                Ok(())
            })
        };
        let completed = match self.process_owner.run_to_completion(RunRequest {
            executable: &executable,
            args: &args,
            cwd: &workspace_root,
            timeout: Duration::from_secs(plan.executor_manifest.timeout_seconds),
            max_output_bytes: plan.executor_manifest.max_output_bytes,
            started: Some(&persist_started),
        }) {
            Ok(completed) => completed,
            Err(error) => {
                self.update_external_effect(
                    &effect_id,
                    ExternalEffectState::OutcomeUnknown,
                    None,
                    Some(format!("QA process outcome is unknown: {error}")),
                )?;
                return Err(error);
            }
        };
        let (output, artifact_bindings) = match interpret_output(
            repo_root,
            &workspace_root,
            &plan.executor_manifest,
            &baseline,
            &completed,
        ) {
            Ok(interpreted) => interpreted,
            Err(error) => {
                let mut fallback = fallback_output(&baseline, &completed);
                fallback
                    .observations
                    .push(format!("QA evidence ingestion failed: {error}"));
                (fallback, Vec::new())
            }
        };
        let result = qa_result(&output);
        let evidence = crate::evidence::bootstrap(repo_root)?.manifest;
        let receipt = ReceiptEnvelope {
            schema_version: 1,
            receipt_version: 2,
            id: plan.receipt_id.clone(),
            kind: ReceiptKind::QaCheckpoint,
            result,
            actor: crate::policy::parse_actor(actor),
            recorded_at: plan.recorded_at,
            subject: SubjectRef {
                kind: "work".to_string(),
                id: saga.ticket_id.clone(),
            },
            bindings: ReceiptBindings {
                source: Some(SourceBinding {
                    kind: "git_commit".to_string(),
                    commit: source_commit.to_string(),
                    repository_id: evidence.repository_id,
                }),
                content: vec![
                    ContentBinding {
                        path: baseline.path.clone(),
                        sha256: baseline.content_hash.clone(),
                    },
                    ContentBinding {
                        path: format!(".pulse/qa/executors/{executor_id}.json"),
                        sha256: plan.executor_manifest_hash.clone(),
                    },
                ],
                artifacts: artifact_bindings,
                ..ReceiptBindings::default()
            },
            payload: ReceiptPayload::QaCheckpoint(QaCheckpointPayload {
                payload_version: 1,
                qa_scope: QaExecutionScope::TicketCheckpoint,
                story_id: baseline.owner_id,
                ticket_id: saga.ticket_id,
                baseline_revision: baseline.revision,
                baseline_content_hash: baseline.content_hash,
                cases: output.cases,
                executor: QaExecutor {
                    name: plan.executor_manifest.id,
                    version: plan.executor_manifest.version,
                    capabilities: plan.executor_manifest.capabilities,
                },
                environment: QaRuntimeEnvironment {
                    profile: plan.executor_manifest.environment_profile,
                    platform: std::env::consts::OS.to_string(),
                    fixture_revision: plan.executor_manifest.fixture_revision,
                },
                observations: output.observations,
                cleanup_passed: output.cleanup_passed,
            }),
        };
        let outcome = crate::evidence::record_receipt_envelope(repo_root, None, receipt)?;
        self.commit_qa_receipt(saga_id, &effect_id, &outcome.receipt)?;
        Ok(DaemonResponse::QaCheckpoint {
            receipt: Box::new(outcome.receipt),
        })
    }

    fn recover_qa_receipt(
        &self,
        saga_id: &str,
        effect_id: &str,
        plan: &QaRunPlan,
        repo_root: &Path,
    ) -> Result<DaemonResponse> {
        crate::evidence::verify_receipt(repo_root, &plan.receipt_id, true, None)?;
        let receipt = crate::evidence::show_receipt(repo_root, &plan.receipt_id)?.receipt;
        let ReceiptPayload::QaCheckpoint(payload) = &receipt.payload else {
            return Err(PulseError::validation(
                "qa_receipt_recovery_conflict",
                "planned QA receipt ID belongs to a different receipt kind",
            ));
        };
        if receipt.kind != ReceiptKind::QaCheckpoint
            || receipt.subject.id != plan.input.ticket_id
            || payload.story_id != plan.input.story_id
            || payload.baseline_revision != plan.input.baseline_revision
            || payload.baseline_content_hash != plan.input.baseline_content_hash
            || receipt
                .bindings
                .source
                .as_ref()
                .map(|source| source.commit.as_str())
                != Some(plan.input.source_commit.as_str())
        {
            return Err(PulseError::validation(
                "qa_receipt_recovery_conflict",
                "planned QA receipt does not match the durable execution plan",
            ));
        }
        self.commit_qa_receipt(saga_id, effect_id, &receipt)?;
        Ok(DaemonResponse::QaCheckpoint {
            receipt: Box::new(receipt),
        })
    }

    fn commit_qa_receipt(
        &self,
        saga_id: &str,
        effect_id: &str,
        receipt: &ReceiptEnvelope,
    ) -> Result<()> {
        self.store.with_state(true, |state| {
            let effect =
                state
                    .external_effects
                    .get_mut(effect_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("external effect {effect_id}"),
                    })?;
            effect.state = ExternalEffectState::Acknowledged;
            effect.resource_id = Some(receipt.id.clone());
            effect.detail = "QA checkpoint receipt recorded".to_string();
            effect.updated_at = Utc::now().to_rfc3339();
            let saga =
                state
                    .assignment_sagas
                    .get_mut(saga_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("assignment saga {saga_id}"),
                    })?;
            if !saga.qa_checkpoint_receipt_ids.contains(&receipt.id) {
                saga.qa_checkpoint_receipt_ids.push(receipt.id.clone());
                saga.qa_checkpoint_receipt_ids.sort();
            }
            saga.updated_at = Utc::now().to_rfc3339();
            let project_id = saga.project_id.clone();
            let workspace_id = saga.workspace_id.clone();
            let session_id = saga.session_id.clone();
            if !state.timeline.iter().any(|event| {
                event.event_type == "assignment.qa_checkpoint_completed"
                    && event
                        .payload
                        .get("receipt_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(receipt.id.as_str())
            }) {
                append_event(
                    state,
                    "assignment.qa_checkpoint_completed",
                    Some(&project_id),
                    workspace_id.as_deref(),
                    session_id.as_deref(),
                    json!({"saga_id": saga_id, "receipt_id": receipt.id, "result": receipt.result}),
                );
            }
            Ok(())
        })
    }
}

fn build_plan(
    executor_root: &Path,
    ticket_id: &str,
    source_commit: &str,
    executor_id: &str,
    baseline: QaBaselineResolution,
    effect_id: &str,
) -> Result<QaRunPlan> {
    let (manifest, executable, manifest_hash) =
        crate::qa::load_executor_manifest(executor_root, executor_id)?;
    let receipt_id = deterministic_receipt_id(effect_id);
    Ok(QaRunPlan {
        schema_version: 1,
        receipt_id,
        recorded_at: Utc::now(),
        executor_manifest: manifest,
        executor_path: executable.to_string_lossy().to_string(),
        executor_manifest_hash: manifest_hash,
        input: QaRunnerInput {
            schema_version: 1,
            story_id: baseline.owner_id,
            ticket_id: ticket_id.to_string(),
            source_commit: source_commit.to_string(),
            baseline_revision: baseline.revision,
            baseline_content_hash: baseline.content_hash,
            cases: baseline.cases,
        },
    })
}

fn resolve_tracked_executable(workspace_root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = crate::storage::safe_repo_relative(relative)?;
    let root =
        fs::canonicalize(workspace_root).map_err(|error| PulseError::io(workspace_root, error))?;
    let candidate = workspace_root.join(relative);
    let metadata =
        fs::symlink_metadata(&candidate).map_err(|error| PulseError::io(&candidate, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PulseError::validation(
            "qa_executor_path_unsafe",
            "QA executor must be a tracked regular file, not a symlink",
        ));
    }
    let executable =
        fs::canonicalize(&candidate).map_err(|error| PulseError::io(&candidate, error))?;
    if !executable.starts_with(root) {
        return Err(PulseError::validation(
            "qa_executor_path_unsafe",
            "QA executor resolves outside the assignment workspace",
        ));
    }
    Ok(executable)
}

fn interpret_output(
    repo_root: &Path,
    workspace_root: &Path,
    manifest: &QaExecutorManifest,
    baseline: &QaBaselineResolution,
    completed: &crate::daemon::process::CompletedProcess,
) -> Result<(QaRunnerOutput, Vec<ArtifactBinding>)> {
    if completed.timed_out || completed.output_truncated || completed.exit_code != Some(0) {
        return Ok((fallback_output(baseline, completed), Vec::new()));
    }
    let output = match serde_json::from_str::<QaRunnerOutput>(completed.stdout.trim()) {
        Ok(output) => output,
        Err(error) => {
            let mut fallback = fallback_output(baseline, completed);
            fallback
                .observations
                .push(format!("invalid structured output: {error}"));
            return Ok((fallback, Vec::new()));
        }
    };
    if let Err(error) = crate::qa::validate_runner_output(manifest, baseline, &output) {
        let mut fallback = fallback_output(baseline, completed);
        fallback
            .observations
            .push(format!("runner contract rejected: {error}"));
        return Ok((fallback, Vec::new()));
    }
    let max_artifact_bytes = crate::evidence::bootstrap(repo_root)?
        .manifest
        .max_artifact_bytes;
    let mut bindings = Vec::new();
    for artifact in &output.artifacts {
        let relative = crate::storage::safe_repo_relative(&artifact.path)?;
        let source = workspace_root.join(relative);
        let metadata =
            fs::symlink_metadata(&source).map_err(|error| PulseError::io(&source, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PulseError::validation(
                "qa_artifact_path_unsafe",
                "QA artifact must be a regular file, not a symlink",
            ));
        }
        let canonical_root = fs::canonicalize(workspace_root)
            .map_err(|error| PulseError::io(workspace_root, error))?;
        let canonical_source =
            fs::canonicalize(&source).map_err(|error| PulseError::io(&source, error))?;
        if !canonical_source.starts_with(canonical_root) {
            return Err(PulseError::validation(
                "qa_artifact_path_unsafe",
                "QA artifact resolves outside the assignment workspace",
            ));
        }
        let outcome = crate::evidence::put_artifact(
            repo_root,
            None,
            &canonical_source,
            artifact.kind.clone(),
            artifact.media_type.clone(),
            None,
            max_artifact_bytes,
        )?;
        bindings.push(ArtifactBinding {
            sha256: outcome.artifact.digest,
            role: artifact.role.clone(),
        });
    }
    Ok((output, bindings))
}

fn fallback_output(
    baseline: &QaBaselineResolution,
    completed: &crate::daemon::process::CompletedProcess,
) -> QaRunnerOutput {
    let detail = if completed.timed_out {
        "QA executor timed out".to_string()
    } else if completed.output_truncated {
        "QA executor output exceeded its configured limit".to_string()
    } else if completed.exit_code != Some(0) {
        format!(
            "QA executor exited with {:?}: {}",
            completed.exit_code,
            completed.stderr.trim()
        )
    } else {
        "QA executor did not produce valid structured output".to_string()
    };
    QaRunnerOutput {
        schema_version: 1,
        cases: baseline
            .cases
            .iter()
            .map(|case| QaCaseObservation {
                case_id: case.id.clone(),
                case_revision: case.revision,
                outcome: QaCaseOutcome::InfrastructureFailure,
            })
            .collect(),
        observations: vec![detail],
        artifacts: Vec::new(),
        cleanup_passed: false,
    }
}

fn qa_result(output: &QaRunnerOutput) -> ReceiptResult {
    if output.cleanup_passed
        && output
            .cases
            .iter()
            .all(|case| case.outcome == QaCaseOutcome::Passed)
    {
        ReceiptResult::Passed
    } else if output.cases.iter().any(|case| {
        matches!(
            case.outcome,
            QaCaseOutcome::ProductFailure | QaCaseOutcome::TestFailure
        )
    }) {
        ReceiptResult::Failed
    } else {
        ReceiptResult::Inconclusive
    }
}

fn deterministic_receipt_id(seed: &str) -> String {
    let digest = crate::canonical_json::hash_bytes(seed.as_bytes());
    let hex = digest.trim_start_matches("sha256:");
    let mut value = 0_u128;
    for character in hex.chars().take(32) {
        value = (value << 4) | u128::from(character.to_digit(16).expect("SHA-256 hex"));
    }
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut encoded = [b'0'; 26];
    for index in (0..26).rev() {
        encoded[index] = ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    format!("rcpt_{}", String::from_utf8_lossy(&encoded))
}
