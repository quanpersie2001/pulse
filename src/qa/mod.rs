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

pub use baseline::{
    load_story_baseline, resolve_ticket_cases, QaBaseline, QaBaselineResolution, QaCase,
    QaCaseApplicability, QaCasePriority,
};
pub use executor::{
    load_executor_manifest, validate_runner_output, QaBrowserAssertion, QaBrowserEngine,
    QaBrowserManifest, QaBrowserReport, QaEnvironmentCommand, QaEnvironmentManifest,
    QaEnvironmentStepOutput, QaExecutorKind, QaExecutorManifest, QaRunnerArtifact, QaRunnerInput,
    QaRunnerOutput,
};
pub use receipt::{
    validate_checkpoint_receipt, QaCaseObservation, QaCaseOutcome, QaCheckpointPayload,
    QaEnvironmentIdentity, QaEnvironmentLifecycle, QaExecutionScope, QaExecutor,
    QaRuntimeEnvironment,
};
