//! Repository-tracked structured QA executor contracts.
//!
//! Core owns parsing and validation of the allowlisted executor declaration and
//! its typed input/output. The daemon owns process execution and lifecycle.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{QaBaselineResolution, QaCaseObservation, QaEnvironmentIdentity};
use crate::canonical_json::hash_bytes;
use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaExecutorManifest {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub kind: QaExecutorKind,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub timeout_seconds: u64,
    pub max_output_bytes: usize,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub environment_profile: String,
    pub fixture_revision: String,
    #[serde(default)]
    pub environment: Option<QaEnvironmentManifest>,
    #[serde(default)]
    pub browser: Option<QaBrowserManifest>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QaExecutorKind {
    #[default]
    Structured,
    Playwright,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaBrowserManifest {
    pub engine: QaBrowserEngine,
    pub base_url: String,
    pub trace_role: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QaBrowserEngine {
    Chromium,
    Firefox,
    Webkit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaEnvironmentManifest {
    pub start: QaEnvironmentCommand,
    pub healthcheck: QaEnvironmentCommand,
    pub reset: QaEnvironmentCommand,
    pub cleanup: QaEnvironmentCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaEnvironmentCommand {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub timeout_seconds: u64,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaEnvironmentStepOutput {
    pub schema_version: u32,
    pub environment_instance_id: String,
    pub source_commit: String,
    pub fixture_revision: String,
    pub observations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaRunnerInput {
    pub schema_version: u32,
    pub story_id: String,
    pub ticket_id: String,
    pub source_commit: String,
    pub baseline_revision: u64,
    pub baseline_content_hash: String,
    pub cases: Vec<super::QaCase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<QaEnvironmentIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaRunnerOutput {
    pub schema_version: u32,
    pub cases: Vec<QaCaseObservation>,
    pub observations: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<QaRunnerArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<QaBrowserReport>,
    pub cleanup_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaBrowserReport {
    pub engine: QaBrowserEngine,
    pub base_url: String,
    pub trace_role: String,
    pub assertions: Vec<QaBrowserAssertion>,
    #[serde(default)]
    pub console_errors: Vec<String>,
    #[serde(default)]
    pub network_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaBrowserAssertion {
    pub case_id: String,
    pub kind: String,
    pub expected: String,
    pub actual: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaRunnerArtifact {
    pub path: String,
    pub role: String,
    #[serde(default = "default_artifact_kind")]
    pub kind: String,
    #[serde(default)]
    pub media_type: Option<String>,
}

fn default_artifact_kind() -> String {
    "qa_evidence".to_string()
}

/// Load an allowlisted executor from `.pulse/qa/executors/<id>.json`.
///
/// # Errors
///
/// Returns a typed validation error for unsafe IDs, malformed manifests, or
/// executable paths that are not repository-relative.
pub fn load_executor_manifest(
    repo_root: &Path,
    executor_id: &str,
) -> Result<(QaExecutorManifest, PathBuf, String)> {
    if executor_id.is_empty()
        || executor_id.len() > 80
        || !executor_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(PulseError::validation(
            "qa_executor_id_invalid",
            "QA executor ID must use only ASCII letters, digits, hyphens, and underscores",
        ));
    }
    let relative = PathBuf::from(format!(".pulse/qa/executors/{executor_id}.json"));
    let path = repo_root.join(&relative);
    let bytes = fs::read(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PulseError::validation(
                "qa_executor_missing",
                format!(
                    "QA executor {executor_id} is not tracked at {}",
                    relative.display()
                ),
            )
        } else {
            PulseError::io(&path, error)
        }
    })?;
    let mut manifest: QaExecutorManifest =
        serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
    normalize(&mut manifest.capabilities);
    if manifest.schema_version != 1
        || manifest.id != executor_id
        || manifest.version.trim().is_empty()
        || manifest.environment_profile.trim().is_empty()
        || manifest.fixture_revision.trim().is_empty()
        || !(1..=900).contains(&manifest.timeout_seconds)
        || !(1_024..=1_048_576).contains(&manifest.max_output_bytes)
        || manifest.args.len() > 32
        || manifest.args.iter().any(|argument| argument.len() > 4_096)
    {
        return Err(PulseError::validation(
            "qa_executor_invalid",
            "QA executor manifest is incomplete or exceeds execution limits",
        ));
    }
    if let Some(environment) = &manifest.environment {
        for command in [
            &environment.start,
            &environment.healthcheck,
            &environment.reset,
            &environment.cleanup,
        ] {
            validate_environment_command(command)?;
        }
    }
    validate_executor_kind(&manifest)?;
    let executable = crate::storage::safe_repo_relative(&manifest.executable)?;
    Ok((manifest, executable, hash_bytes(&bytes)))
}

fn validate_executor_kind(manifest: &QaExecutorManifest) -> Result<()> {
    match (manifest.kind, manifest.browser.as_ref()) {
        (QaExecutorKind::Structured, None) => Ok(()),
        (QaExecutorKind::Structured, Some(_)) => Err(PulseError::validation(
            "qa_executor_browser_unexpected",
            "structured QA executors cannot declare a browser contract",
        )),
        (QaExecutorKind::Playwright, Some(browser)) => {
            let capabilities = manifest
                .capabilities
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if manifest.environment.is_none()
                || !["browser", "deterministic-assertion", "playwright"]
                    .iter()
                    .all(|required| capabilities.contains(required))
                || browser.base_url.len() > 2_048
                || !(browser.base_url.starts_with("http://")
                    || browser.base_url.starts_with("https://"))
                || browser.base_url.chars().any(char::is_whitespace)
                || !is_portable_token(&browser.trace_role)
            {
                return Err(PulseError::validation(
                    "qa_playwright_executor_invalid",
                    "Playwright executors require lifecycle, browser/playwright/deterministic-assertion capabilities, an HTTP base URL, and a portable trace role",
                ));
            }
            Ok(())
        }
        (QaExecutorKind::Playwright, None) => Err(PulseError::validation(
            "qa_playwright_executor_invalid",
            "Playwright executors require a browser contract",
        )),
    }
}

fn validate_environment_command(command: &QaEnvironmentCommand) -> Result<()> {
    crate::storage::safe_repo_relative(&command.executable)?;
    if !(1..=900).contains(&command.timeout_seconds)
        || !(1_024..=1_048_576).contains(&command.max_output_bytes)
        || command.args.len() > 32
        || command.args.iter().any(|argument| argument.len() > 4_096)
    {
        return Err(PulseError::validation(
            "qa_environment_command_invalid",
            "QA environment command exceeds argument, timeout, or output limits",
        ));
    }
    Ok(())
}

/// Validate that runner output covers exactly the resolved case revisions and
/// supplies every required capability and evidence role.
///
/// # Errors
///
/// Returns a typed validation error for incomplete, duplicate, stale, or
/// unsupported output.
pub fn validate_runner_output(
    manifest: &QaExecutorManifest,
    baseline: &QaBaselineResolution,
    output: &QaRunnerOutput,
) -> Result<()> {
    if output.schema_version != 1
        || output.cases.is_empty()
        || output
            .observations
            .iter()
            .all(|observation| observation.trim().is_empty())
    {
        return Err(PulseError::validation(
            "qa_runner_output_invalid",
            "QA runner output is incomplete or uses an unsupported version",
        ));
    }
    let expected = baseline
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut actual = BTreeSet::new();
    for observation in &output.cases {
        let case = expected.get(observation.case_id.as_str()).ok_or_else(|| {
            PulseError::validation(
                "qa_runner_case_unexpected",
                format!("QA runner returned unselected case {}", observation.case_id),
            )
        })?;
        if !actual.insert(observation.case_id.as_str())
            || observation.case_revision != case.revision
        {
            return Err(PulseError::validation(
                "qa_runner_case_stale",
                format!(
                    "QA runner case {} is duplicated or stale",
                    observation.case_id
                ),
            ));
        }
    }
    if actual != expected.keys().copied().collect() {
        return Err(PulseError::validation(
            "qa_runner_coverage_incomplete",
            "QA runner output must cover exactly the selected cases",
        ));
    }
    validate_browser_output(manifest, baseline, output)?;
    let capabilities = manifest
        .capabilities
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let roles = output
        .artifacts
        .iter()
        .map(|artifact| artifact.role.as_str())
        .collect::<BTreeSet<_>>();
    for case in &baseline.cases {
        if !case
            .required_capabilities
            .iter()
            .all(|value| capabilities.contains(value.as_str()))
        {
            return Err(PulseError::validation(
                "qa_executor_capability_missing",
                format!("executor lacks a capability required by {}", case.id),
            ));
        }
        if !case
            .required_evidence
            .iter()
            .all(|value| roles.contains(value.as_str()))
        {
            return Err(PulseError::validation(
                "qa_runner_evidence_missing",
                format!(
                    "runner output lacks an evidence role required by {}",
                    case.id
                ),
            ));
        }
    }
    Ok(())
}

fn validate_browser_output(
    manifest: &QaExecutorManifest,
    baseline: &QaBaselineResolution,
    output: &QaRunnerOutput,
) -> Result<()> {
    match (
        manifest.kind,
        manifest.browser.as_ref(),
        output.browser.as_ref(),
    ) {
        (QaExecutorKind::Structured, None, None) => Ok(()),
        (QaExecutorKind::Playwright, Some(contract), Some(report)) => {
            if report.engine != contract.engine
                || report.base_url != contract.base_url
                || report.trace_role != contract.trace_role
                || report.assertions.is_empty()
                || report.assertions.len() > 1_024
                || report.console_errors.len() > 1_024
                || report.network_errors.len() > 1_024
                || report
                    .console_errors
                    .iter()
                    .chain(&report.network_errors)
                    .any(|value| value.trim().is_empty() || value.len() > 8_192)
                || !output
                    .artifacts
                    .iter()
                    .any(|artifact| artifact.role == contract.trace_role)
                || baseline.cases.iter().any(|case| case.surface != "web")
            {
                return Err(PulseError::validation(
                    "qa_playwright_output_invalid",
                    "Playwright output must match its browser contract, cover only web cases, and include the declared trace artifact",
                ));
            }
            let outcomes = output
                .cases
                .iter()
                .map(|case| (case.case_id.as_str(), case.outcome))
                .collect::<std::collections::BTreeMap<_, _>>();
            let mut covered = BTreeSet::new();
            let mut assertion_keys = BTreeSet::new();
            for assertion in &report.assertions {
                let Some(outcome) = outcomes.get(assertion.case_id.as_str()) else {
                    return Err(PulseError::validation(
                        "qa_playwright_assertion_invalid",
                        format!(
                            "browser assertion references unselected case {}",
                            assertion.case_id
                        ),
                    ));
                };
                if !assertion_keys.insert((assertion.case_id.as_str(), assertion.kind.as_str()))
                    || assertion.kind.trim().is_empty()
                    || assertion.expected.trim().is_empty()
                    || assertion.actual.trim().is_empty()
                    || assertion.kind.len() > 128
                    || assertion.expected.len() > 8_192
                    || assertion.actual.len() > 8_192
                    || (*outcome == super::QaCaseOutcome::Passed && !assertion.passed)
                {
                    return Err(PulseError::validation(
                        "qa_playwright_assertion_invalid",
                        "browser assertions must be unique, bounded, complete, and consistent with passed case outcomes",
                    ));
                }
                covered.insert(assertion.case_id.as_str());
            }
            if covered != outcomes.keys().copied().collect() {
                return Err(PulseError::validation(
                    "qa_playwright_coverage_incomplete",
                    "Playwright output needs at least one deterministic assertion for every selected case",
                ));
            }
            Ok(())
        }
        _ => Err(PulseError::validation(
            "qa_executor_output_kind_mismatch",
            "QA runner output does not match the executor kind",
        )),
    }
}

fn is_portable_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn normalize(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.sort();
    values.dedup();
}
