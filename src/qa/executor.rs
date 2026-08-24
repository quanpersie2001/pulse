//! Repository-tracked structured QA executor contracts.
//!
//! Core owns parsing and validation of the allowlisted executor declaration and
//! its typed input/output. The daemon owns process execution and lifecycle.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{QaBaselineResolution, QaCaseObservation};
use crate::canonical_json::hash_bytes;
use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaExecutorManifest {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub timeout_seconds: u64,
    pub max_output_bytes: usize,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub environment_profile: String,
    pub fixture_revision: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaRunnerOutput {
    pub schema_version: u32,
    pub cases: Vec<QaCaseObservation>,
    pub observations: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<QaRunnerArtifact>,
    pub cleanup_passed: bool,
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
    let executable = crate::storage::safe_repo_relative(&manifest.executable)?;
    Ok((manifest, executable, hash_bytes(&bytes)))
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

fn normalize(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.sort();
    values.dedup();
}
