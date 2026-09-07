//! Core-owned behavioral QA contracts and deterministic Story baseline resolution.
//!
//! A Story owns `works/<STORY-ID>/qa.md`, a markdown document with conventional
//! headings (Decision 0010). Prose explains intent; readiness and close gates
//! consume the parsed contract, bind receipts to the hash of the complete file
//! and each case to the hash of its own section. JSON exists only at the runner
//! boundary. This module performs no execution and has no daemon dependency.

mod baseline;
mod executor;
mod receipt;

pub use baseline::{
    baseline_template, load_story_baseline, parse_baseline, resolve_story_cases,
    resolve_ticket_cases, QaAssertion, QaBaseline, QaBaselinePosture, QaBaselineResolution, QaCase,
    QaCaseApplicability, QaCasePriority, QaCaseSurface, QaCheck, QaRisk,
};
pub use executor::{
    load_executor_manifest, validate_runner_output, QaExecutorManifest, QaRunnerArtifact,
    QaRunnerInput, QaRunnerOutput,
};
pub use receipt::{
    build_checkpoint_envelope, validate_checkpoint_receipt, QaCaseObservation, QaCaseOutcome,
    QaCheckpointPayload, QaExecutionScope, QaExecutor,
};
