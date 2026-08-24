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
use crate::daemon::process::{CompletedProcess, RunRequest};
use crate::daemon::protocol::DaemonResponse;
use crate::evidence::model::{
    ArtifactBinding, ContentBinding, ReceiptBindings, ReceiptEnvelope, ReceiptKind, ReceiptPayload,
    ReceiptResult, SourceBinding, SubjectRef,
};
use crate::qa::{
    QaBaselineResolution, QaCaseObservation, QaCaseOutcome, QaCheckpointPayload,
    QaEnvironmentCommand, QaEnvironmentIdentity, QaEnvironmentLifecycle, QaEnvironmentManifest,
    QaEnvironmentStepOutput, QaExecutionScope, QaExecutor, QaExecutorManifest, QaFlakyWaiver,
    QaQualificationContext, QaRunnerInput, QaRunnerOutput, QaRunnerQualification,
    QaRuntimeEnvironment,
};
use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QaRunPlan {
    schema_version: u32,
    #[serde(default)]
    scope: QaExecutionScope,
    receipt_id: String,
    recorded_at: chrono::DateTime<Utc>,
    executor_manifest: QaExecutorManifest,
    executor_path: String,
    executor_manifest_hash: String,
    input: QaRunnerInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    qualification: Option<QaQualificationContext>,
}

struct EnvironmentProgress {
    identity: QaEnvironmentIdentity,
    start_passed: bool,
    healthcheck_passed: bool,
    reset_passed: bool,
    cleanup_passed: bool,
    observations: Vec<String>,
}

impl EnvironmentProgress {
    fn ready(&self) -> bool {
        self.start_passed && self.healthcheck_passed && self.reset_passed
    }

    fn receipt(self) -> QaEnvironmentLifecycle {
        QaEnvironmentLifecycle {
            identity: self.identity,
            start_passed: self.start_passed,
            healthcheck_passed: self.healthcheck_passed,
            reset_passed: self.reset_passed,
            cleanup_passed: self.cleanup_passed,
        }
    }
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
        self.qa_run(
            saga_id,
            None,
            actor,
            source_commit,
            executor_id,
            idempotency_key,
            QaExecutionScope::TicketCheckpoint,
            None,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn qa_story_qualification_run(
        &self,
        saga_id: &str,
        story_id: &str,
        actor: &str,
        source_commit: &str,
        executor_id: &str,
        matrix_entry_id: &str,
        retry_of: Option<&str>,
        waiver_reason: Option<&str>,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        self.qa_run(
            saga_id,
            Some(story_id),
            actor,
            source_commit,
            executor_id,
            idempotency_key,
            QaExecutionScope::StoryClose,
            Some(matrix_entry_id),
            retry_of,
            waiver_reason,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn qa_run(
        &self,
        saga_id: &str,
        story_id: Option<&str>,
        actor: &str,
        source_commit: &str,
        executor_id: &str,
        idempotency_key: &str,
        scope: QaExecutionScope,
        matrix_entry_id: Option<&str>,
        retry_of: Option<&str>,
        waiver_reason: Option<&str>,
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
        let baseline = match scope {
            QaExecutionScope::TicketCheckpoint => {
                crate::qa::resolve_ticket_cases(repo_root, &ticket)?
            }
            QaExecutionScope::StoryClose => {
                let story_id = story_id.ok_or_else(|| {
                    PulseError::validation(
                        "qa_story_id_missing",
                        "Story qualification requires a Story ID",
                    )
                })?;
                let owner = ticket
                    .qa
                    .as_ref()
                    .and_then(|qa| qa.impact.behavioral_owner.as_deref());
                if owner != Some(story_id) {
                    return Err(PulseError::validation(
                        "qa_story_assignment_mismatch",
                        "Story qualification must use an assignment bound to that behavioral owner",
                    ));
                }
                crate::qa::resolve_story_matrix_entry(
                    repo_root,
                    story_id,
                    matrix_entry_id.unwrap_or("default"),
                )?
            }
        };
        let qualification = build_qualification_context(
            repo_root,
            scope,
            actor,
            source_commit,
            matrix_entry_id,
            retry_of,
            waiver_reason,
            &baseline,
        )?;
        let effect_id = deterministic_id("effect_qa", &format!("{saga_id}:{idempotency_key}"));
        let request_fingerprint = hash_serializable(&(
            saga_id,
            scope,
            actor,
            source_commit,
            executor_id,
            &baseline.content_hash,
            &qualification,
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
                scope,
                qualification.clone(),
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
            format!("run {scope:?} QA executor {executor_id}"),
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
            Some("QA checkpoint lifecycle started".to_string()),
        )?;
        let mut environment = if let Some(contract) = &plan.executor_manifest.environment {
            Some(self.prepare_environment(
                &effect_id,
                &workspace_root,
                &input_path,
                contract,
                source_commit,
                &plan.executor_manifest.fixture_revision,
            )?)
        } else {
            None
        };
        let (mut output, artifact_bindings) = if environment
            .as_ref()
            .map_or(true, EnvironmentProgress::ready)
        {
            let mut runner_input = plan.input.clone();
            runner_input.environment = environment
                .as_ref()
                .map(|progress| progress.identity.clone());
            crate::storage::atomic_write_private(&input_path, &to_canonical_bytes(&runner_input)?)?;
            let executable = resolve_tracked_executable(&workspace_root, &plan.executor_path)?;
            let completed = self.run_qa_command(
                &effect_id,
                "execute",
                &workspace_root,
                &input_path,
                &executable,
                &plan.executor_manifest.args,
                plan.executor_manifest.timeout_seconds,
                plan.executor_manifest.max_output_bytes,
            )?;
            match interpret_output(
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
            }
        } else {
            (
                infrastructure_output(
                    &baseline,
                    "QA executor was skipped because environment preparation failed",
                ),
                Vec::new(),
            )
        };
        if let (Some(progress), Some(contract)) = (
            environment.as_mut(),
            plan.executor_manifest.environment.as_ref(),
        ) {
            self.cleanup_environment(
                &effect_id,
                &workspace_root,
                &input_path,
                &contract.cleanup,
                source_commit,
                &plan.executor_manifest.fixture_revision,
                progress,
            )?;
            output.cleanup_passed &= progress.cleanup_passed;
            output.observations.extend(progress.observations.clone());
        }
        let result = qa_result(&output);
        let browser = output.browser.clone();
        let payload_version = if browser.is_some() {
            3
        } else if environment.is_some() {
            2
        } else {
            1
        };
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
                id: match plan.scope {
                    QaExecutionScope::TicketCheckpoint => saga.ticket_id.clone(),
                    QaExecutionScope::StoryClose => baseline.owner_id.clone(),
                },
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
                payload_version,
                qa_scope: plan.scope,
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
                    lifecycle: environment.map(EnvironmentProgress::receipt),
                },
                browser,
                qualification: plan.qualification.clone(),
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

    #[allow(clippy::too_many_arguments)]
    fn run_qa_command(
        &self,
        effect_id: &str,
        phase: &str,
        workspace_root: &Path,
        input_path: &Path,
        executable: &Path,
        fixed_args: &[String],
        timeout_seconds: u64,
        max_output_bytes: usize,
    ) -> Result<CompletedProcess> {
        self.update_external_effect(
            effect_id,
            ExternalEffectState::Attempting,
            None,
            Some(format!("QA {phase} process started")),
        )?;
        let mut args = fixed_args.to_vec();
        args.push(input_path.to_string_lossy().to_string());
        let persist_started = |record: &crate::daemon::process::HelperProcessRecord| {
            self.store.with_state(true, |state| {
                let effect = state.external_effects.get_mut(effect_id).ok_or_else(|| {
                    PulseError::NotFound {
                        subject: format!("external effect {effect_id}"),
                    }
                })?;
                effect.attempt_process = Some(record.clone());
                effect.detail = format!("QA {phase} process identity persisted");
                effect.updated_at = Utc::now().to_rfc3339();
                Ok(())
            })
        };
        match self.process_owner.run_to_completion(RunRequest {
            executable,
            args: &args,
            cwd: workspace_root,
            timeout: Duration::from_secs(timeout_seconds),
            max_output_bytes,
            started: Some(&persist_started),
        }) {
            Ok(completed) => Ok(completed),
            Err(error) => {
                self.update_external_effect(
                    effect_id,
                    ExternalEffectState::OutcomeUnknown,
                    None,
                    Some(format!("QA {phase} process outcome is unknown: {error}")),
                )?;
                Err(error)
            }
        }
    }

    fn prepare_environment(
        &self,
        effect_id: &str,
        workspace_root: &Path,
        input_path: &Path,
        contract: &QaEnvironmentManifest,
        source_commit: &str,
        fixture_revision: &str,
    ) -> Result<EnvironmentProgress> {
        let mut progress = EnvironmentProgress {
            identity: QaEnvironmentIdentity {
                environment_instance_id: format!("unresolved:{effect_id}"),
                source_commit: source_commit.to_string(),
                fixture_revision: fixture_revision.to_string(),
            },
            start_passed: false,
            healthcheck_passed: false,
            reset_passed: false,
            cleanup_passed: false,
            observations: Vec::new(),
        };
        match self.run_environment_step(
            effect_id,
            "environment_start",
            workspace_root,
            input_path,
            &contract.start,
            source_commit,
            fixture_revision,
            None,
        )? {
            Ok(output) => {
                progress.identity = environment_identity(&output);
                progress.start_passed = true;
                append_step_observations(&mut progress.observations, "start", &output);
            }
            Err(reason) => {
                progress
                    .observations
                    .push(format!("start failed: {reason}"));
                return Ok(progress);
            }
        }
        match self.run_environment_step(
            effect_id,
            "environment_healthcheck",
            workspace_root,
            input_path,
            &contract.healthcheck,
            source_commit,
            fixture_revision,
            Some(&progress.identity),
        )? {
            Ok(output) => {
                progress.healthcheck_passed = true;
                append_step_observations(&mut progress.observations, "healthcheck", &output);
            }
            Err(reason) => {
                progress
                    .observations
                    .push(format!("healthcheck failed: {reason}"));
                return Ok(progress);
            }
        }
        match self.run_environment_step(
            effect_id,
            "environment_reset",
            workspace_root,
            input_path,
            &contract.reset,
            source_commit,
            fixture_revision,
            Some(&progress.identity),
        )? {
            Ok(output) => {
                progress.reset_passed = true;
                append_step_observations(&mut progress.observations, "reset", &output);
            }
            Err(reason) => progress
                .observations
                .push(format!("reset failed: {reason}")),
        }
        Ok(progress)
    }

    #[allow(clippy::too_many_arguments)]
    fn cleanup_environment(
        &self,
        effect_id: &str,
        workspace_root: &Path,
        input_path: &Path,
        command: &QaEnvironmentCommand,
        source_commit: &str,
        fixture_revision: &str,
        progress: &mut EnvironmentProgress,
    ) -> Result<()> {
        match self.run_environment_step(
            effect_id,
            "environment_cleanup",
            workspace_root,
            input_path,
            command,
            source_commit,
            fixture_revision,
            progress.start_passed.then_some(&progress.identity),
        )? {
            Ok(output) => {
                if !progress.start_passed {
                    progress.identity = environment_identity(&output);
                }
                progress.cleanup_passed = true;
                append_step_observations(&mut progress.observations, "cleanup", &output);
            }
            Err(reason) => progress
                .observations
                .push(format!("cleanup failed: {reason}")),
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn run_environment_step(
        &self,
        effect_id: &str,
        phase: &str,
        workspace_root: &Path,
        input_path: &Path,
        command: &QaEnvironmentCommand,
        source_commit: &str,
        fixture_revision: &str,
        expected_identity: Option<&QaEnvironmentIdentity>,
    ) -> Result<std::result::Result<QaEnvironmentStepOutput, String>> {
        let executable = resolve_tracked_executable(workspace_root, &command.executable)?;
        let completed = self.run_qa_command(
            effect_id,
            phase,
            workspace_root,
            input_path,
            &executable,
            &command.args,
            command.timeout_seconds,
            command.max_output_bytes,
        )?;
        if completed.timed_out || completed.output_truncated || completed.exit_code != Some(0) {
            return Ok(Err(completed_failure(&completed)));
        }
        let output = match serde_json::from_str::<QaEnvironmentStepOutput>(completed.stdout.trim())
        {
            Ok(output) => output,
            Err(error) => return Ok(Err(format!("invalid structured output: {error}"))),
        };
        if output.schema_version != 1
            || output.environment_instance_id.trim().is_empty()
            || output.source_commit != source_commit
            || output.fixture_revision != fixture_revision
            || output
                .observations
                .iter()
                .all(|observation| observation.trim().is_empty())
            || expected_identity.is_some_and(|identity| {
                identity.environment_instance_id != output.environment_instance_id
                    || identity.source_commit != output.source_commit
                    || identity.fixture_revision != output.fixture_revision
            })
        {
            return Ok(Err(
                "environment identity, fixture, source, or observations do not match".to_string(),
            ));
        }
        Ok(Ok(output))
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
            || receipt.subject.id
                != match plan.scope {
                    QaExecutionScope::TicketCheckpoint => plan.input.ticket_id.as_str(),
                    QaExecutionScope::StoryClose => plan.input.story_id.as_str(),
                }
            || payload.qa_scope != plan.scope
            || payload.story_id != plan.input.story_id
            || payload.baseline_revision != plan.input.baseline_revision
            || payload.baseline_content_hash != plan.input.baseline_content_hash
            || payload.qualification != plan.qualification
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

#[allow(clippy::too_many_arguments)]
fn build_plan(
    executor_root: &Path,
    ticket_id: &str,
    source_commit: &str,
    executor_id: &str,
    baseline: QaBaselineResolution,
    effect_id: &str,
    scope: QaExecutionScope,
    qualification: Option<QaQualificationContext>,
) -> Result<QaRunPlan> {
    let (manifest, executable, manifest_hash) =
        crate::qa::load_executor_manifest(executor_root, executor_id)?;
    let receipt_id = deterministic_receipt_id(effect_id);
    if let (Some(context), Some(entry)) = (&qualification, baseline.matrix.first()) {
        if context.matrix_entry_id != entry.id
            || manifest.environment_profile != entry.environment_profile
            || (entry.platform != "any" && entry.platform != std::env::consts::OS)
        {
            return Err(PulseError::validation(
                "qa_qualification_matrix_mismatch",
                "QA executor environment does not satisfy the selected matrix entry",
            ));
        }
    }
    let runner_qualification = qualification.as_ref().map(|context| QaRunnerQualification {
        matrix_entry_id: context.matrix_entry_id.clone(),
        attempt: context.attempt,
        previous_attempt_receipt_id: context.previous_attempt_receipt_id.clone(),
    });
    Ok(QaRunPlan {
        schema_version: 1,
        scope,
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
            qualification: runner_qualification,
            environment: None,
        },
        qualification,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_qualification_context(
    repo_root: &Path,
    scope: QaExecutionScope,
    actor: &str,
    source_commit: &str,
    matrix_entry_id: Option<&str>,
    retry_of: Option<&str>,
    waiver_reason: Option<&str>,
    baseline: &QaBaselineResolution,
) -> Result<Option<QaQualificationContext>> {
    if scope != QaExecutionScope::StoryClose {
        if retry_of.is_some() || waiver_reason.is_some() {
            return Err(PulseError::validation(
                "qa_retry_scope_invalid",
                "retry lineage and flaky waivers are currently defined for Story qualification",
            ));
        }
        return Ok(None);
    }
    let matrix_entry_id = matrix_entry_id.unwrap_or("default");
    let (attempt, previous_attempt_receipt_id) = match retry_of {
        Some(receipt_id) => {
            crate::evidence::verify_receipt(repo_root, receipt_id, true, None)?;
            let previous = crate::evidence::show_receipt(repo_root, receipt_id)?.receipt;
            let ReceiptPayload::QaCheckpoint(payload) = &previous.payload else {
                return Err(PulseError::validation(
                    "qa_retry_receipt_invalid",
                    "retry predecessor must be a Story qualification receipt",
                ));
            };
            let context = payload.qualification.as_ref().ok_or_else(|| {
                PulseError::validation(
                    "qa_retry_receipt_invalid",
                    "retry predecessor has no attempt lineage",
                )
            })?;
            if previous.kind != ReceiptKind::QaCheckpoint
                || previous.result == ReceiptResult::Passed
                || payload.qa_scope != QaExecutionScope::StoryClose
                || payload.story_id != baseline.owner_id
                || payload.baseline_revision != baseline.revision
                || payload.baseline_content_hash != baseline.content_hash
                || context.matrix_entry_id != matrix_entry_id
                || previous
                    .bindings
                    .source
                    .as_ref()
                    .map(|source| source.commit.as_str())
                    != Some(source_commit)
            {
                return Err(PulseError::validation(
                    "qa_retry_receipt_invalid",
                    "retry predecessor does not match the current Story, source, baseline, and matrix entry",
                ));
            }
            (context.attempt + 1, Some(receipt_id.to_string()))
        }
        None => {
            if waiver_reason.is_some() {
                return Err(PulseError::validation(
                    "qa_flaky_waiver_invalid",
                    "a flaky waiver can only approve an explicit retry lineage",
                ));
            }
            (1, None)
        }
    };
    let flaky_waiver = match waiver_reason {
        Some(rationale) => {
            if rationale.trim().is_empty() {
                return Err(PulseError::validation(
                    "qa_flaky_waiver_invalid",
                    "a flaky waiver requires a non-empty rationale",
                ));
            }
            let report = crate::policy::load_authority_policy(repo_root)?;
            let approved_by = crate::policy::parse_actor(actor);
            crate::policy::authorize(&report, &approved_by, &["qa.flaky.waive"])?;
            Some(QaFlakyWaiver {
                rationale: rationale.trim().to_string(),
                approved_by,
                policy_revision: report.policy_revision.ok_or_else(|| {
                    PulseError::validation(
                        "qa_flaky_waiver_invalid",
                        "authority policy revision is unavailable",
                    )
                })?,
                policy_fingerprint: report.fingerprint.ok_or_else(|| {
                    PulseError::validation(
                        "qa_flaky_waiver_invalid",
                        "authority policy fingerprint is unavailable",
                    )
                })?,
            })
        }
        None => None,
    };
    Ok(Some(QaQualificationContext {
        matrix_entry_id: matrix_entry_id.to_string(),
        attempt,
        previous_attempt_receipt_id,
        flaky_waiver,
    }))
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
        browser: None,
        cleanup_passed: false,
    }
}

fn infrastructure_output(baseline: &QaBaselineResolution, detail: &str) -> QaRunnerOutput {
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
        observations: vec![detail.to_string()],
        artifacts: Vec::new(),
        browser: None,
        cleanup_passed: false,
    }
}

fn completed_failure(completed: &CompletedProcess) -> String {
    if completed.timed_out {
        "command timed out".to_string()
    } else if completed.output_truncated {
        "command output exceeded its configured limit".to_string()
    } else {
        format!(
            "command exited with {:?}: {}",
            completed.exit_code,
            completed.stderr.trim()
        )
    }
}

fn environment_identity(output: &QaEnvironmentStepOutput) -> QaEnvironmentIdentity {
    QaEnvironmentIdentity {
        environment_instance_id: output.environment_instance_id.clone(),
        source_commit: output.source_commit.clone(),
        fixture_revision: output.fixture_revision.clone(),
    }
}

fn append_step_observations(
    target: &mut Vec<String>,
    phase: &str,
    output: &QaEnvironmentStepOutput,
) {
    target.extend(
        output
            .observations
            .iter()
            .map(|observation| format!("{phase}: {observation}")),
    );
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
