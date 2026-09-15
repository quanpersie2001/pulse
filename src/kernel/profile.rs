//! `PULSE.md` profile lookup (plan 0022 §8.1).
//!
//! Pulled forward from P1.9 into P1.7: the close gate (§7.3 condition 3)
//! needs to know which lanes a Ticket's `surface-risk` profile requires, and
//! close is P1.7's job. Lane execution/sealing (`kernel::lane`,
//! `kernel::run`) is still P1.9's.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::{PulseError, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub lanes: Vec<String>,
    /// `Some("required")` when a human must run `close` (plan §7.3
    /// condition 4); absent otherwise.
    #[serde(default)]
    pub human: Option<String>,
}

impl Profile {
    pub fn human_required(&self) -> bool {
        self.human.as_deref() == Some("required")
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PulseConfig {
    #[serde(default)]
    pub fence_ignore: Vec<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

/// Read and parse `PULSE.md` at the repo root. `#` lines are YAML comments
/// (the markdown-looking banner `pulse init` seeds is already valid YAML),
/// so no markdown stripping is needed.
///
/// # Errors
/// `pulse_md_invalid` if the file is missing or not valid YAML.
pub fn load(repo_root: &Path) -> Result<PulseConfig> {
    let path = repo_root.join("PULSE.md");
    let text = std::fs::read_to_string(&path).map_err(|error| {
        PulseError::kernel(
            "pulse_md_invalid",
            format!("could not read {}: {error}", path.display()),
            "run `pulse init` to seed PULSE.md",
        )
    })?;
    serde_yaml::from_str(&text).map_err(|error| {
        PulseError::kernel(
            "pulse_md_invalid",
            format!("PULSE.md is not valid YAML: {error}"),
            "PULSE.md must be YAML with a `profiles:` map and optional `fence_ignore:`",
        )
    })
}

/// The profile key for a Ticket: `decision_work` role tickets use that
/// literal key; every other ticket uses `<surface>-<risk>` (plan §8.1).
pub fn profile_key(role: &str, surface: Option<&str>, risk: Option<&str>) -> String {
    if role == "decision_work" {
        return "decision_work".to_string();
    }
    format!(
        "{}-{}",
        surface.unwrap_or("unknown"),
        risk.unwrap_or("unknown")
    )
}

/// # Errors
/// `profile_missing` if `key` has no entry in `config.profiles`.
pub fn profile_for<'a>(config: &'a PulseConfig, key: &str) -> Result<&'a Profile> {
    config.profiles.get(key).ok_or_else(|| {
        PulseError::kernel(
            "profile_missing",
            format!("no profile for {key} in PULSE.md"),
            "add a `{key}: {lanes: [...]}` entry to PULSE.md's profiles map",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_pulse_md(repo_root: &Path, body: &str) {
        std::fs::write(repo_root.join("PULSE.md"), body).unwrap();
    }

    #[test]
    fn parses_the_default_seed_shape() {
        let repo = tempfile::tempdir().unwrap();
        write_pulse_md(
            repo.path(),
            "# PULSE.md - seeded by `pulse init`.\nfence_ignore: []\nprofiles:\n  cli-low:\n    lanes: [review-correctness]\n",
        );
        let config = load(repo.path()).unwrap();
        assert!(config.fence_ignore.is_empty());
        let profile = profile_for(&config, "cli-low").unwrap();
        assert_eq!(profile.lanes, vec!["review-correctness".to_string()]);
        assert!(!profile.human_required());
    }

    #[test]
    fn missing_profile_key_is_reported_with_a_hint() {
        let repo = tempfile::tempdir().unwrap();
        write_pulse_md(repo.path(), "profiles: {}\n");
        let config = load(repo.path()).unwrap();
        let err = profile_for(&config, "api-high").unwrap_err();
        assert_eq!(err.code(), "profile_missing");
        assert!(err.hint().is_some());
    }

    #[test]
    fn human_required_profile_is_recognised() {
        let repo = tempfile::tempdir().unwrap();
        write_pulse_md(
            repo.path(),
            "profiles:\n  api-high:\n    lanes: [review-correctness]\n    human: required\n",
        );
        let config = load(repo.path()).unwrap();
        assert!(profile_for(&config, "api-high").unwrap().human_required());
    }

    #[test]
    fn decision_work_uses_its_own_key_regardless_of_surface_or_risk() {
        assert_eq!(
            profile_key("decision_work", Some("cli"), Some("high")),
            "decision_work"
        );
        assert_eq!(
            profile_key("implementation", Some("api"), Some("low")),
            "api-low"
        );
    }
}
