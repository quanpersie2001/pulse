//! Core-owned behavioral QA contracts and deterministic Story baseline resolution.
//!
//! A Story owns `works/<STORY-ID>/qa.md`. The machine-readable contract is a
//! single `pulse-qa` fenced JSON block inside that document; prose may explain
//! intent, but readiness and close gates consume only the typed block and bind
//! receipts to the hash of the complete file. This module performs no execution
//! and has no daemon dependency.

mod baseline;
mod executor;
mod receipt;

pub(crate) use baseline::validate_approval;

pub use baseline::{
    load_story_baseline, resolve_story_cases, resolve_story_matrix_entry, resolve_ticket_cases,
    QaAuthorityApproval, QaBaseline, QaBaselineResolution, QaCase, QaCaseApplicability,
    QaCasePriority, QaMatrixEntry,
};
pub use executor::{
    load_executor_manifest, validate_runner_output, QaBrowserAssertion, QaBrowserEngine,
    QaBrowserManifest, QaBrowserReport, QaEnvironmentCommand, QaEnvironmentManifest,
    QaEnvironmentStepOutput, QaExecutorKind, QaExecutorManifest, QaRunnerArtifact, QaRunnerInput,
    QaRunnerOutput, QaRunnerQualification,
};
pub use receipt::{
    validate_checkpoint_receipt, validate_current_deployment_binding, QaCaseObservation,
    QaCaseOutcome, QaCheckpointPayload, QaDeploymentIdentity, QaEnvironmentIdentity,
    QaEnvironmentLifecycle, QaExecutionScope, QaExecutor, QaFlakyWaiver, QaQualificationContext,
    QaRuntimeEnvironment,
};
