//! `PULSE.md` profile lookup (plan 0022 §8.1).
//!
//! Pulled forward from P1.9 into P1.7: the close gate (§7.3 condition 3)
//! needs to know which lanes a Ticket's `surface-risk` profile requires, and
//! close is P1.7's job. Lane execution/sealing (`kernel::lane`,
//! `kernel::run`) is still P1.9's.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::error::{PulseError, Result};
use crate::kernel::reservation::touches_of;
use crate::source::{self, Source};

#[derive(Debug, Clone, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub lanes: Vec<String>,
    /// `Some("required")` when a human must run `close` (plan §7.3
    /// condition 4); absent otherwise.
    #[serde(default)]
    pub human: Option<String>,
    /// Opt-in independent review panels, keyed by lane role (decision
    /// 0027 C1). Absent means every lane is the single lane it has always
    /// been.
    #[serde(default)]
    pub panels: BTreeMap<String, Panel>,
}

/// How many independent reviewers one lane runs as, and how many must agree
/// (decision 0027). `count` is `>= 2`; a panel of one is just the lane.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Panel {
    pub count: u32,
    pub quorum: u32,
}

impl Profile {
    pub fn human_required(&self) -> bool {
        self.human.as_deref() == Some("required")
    }

    /// The panel `role` runs as in this profile, when one is declared
    /// (decision 0027 C1).
    pub fn panel(&self, role: &str) -> Option<&Panel> {
        self.panels.get(role)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PulseConfig {
    #[serde(default)]
    pub fence_ignore: Vec<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
    /// Pre-edit hook behaviour (plan 0025 G1). Absent means the default —
    /// an edit with no active ticket is allowed, so installing the hook
    /// alone changes nothing until a ticket is claimed.
    #[serde(default)]
    pub hook: HookConfig,
}

/// What an edit may do when no ticket holds a live lease (plan 0025 G1).
/// `Allow` (the default) keeps an unenrolled-feeling repo fully editable;
/// `Deny` turns the hook into "claim before you edit" for every path the
/// reservation rules would otherwise leave outside all `touches`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnclaimedPolicy {
    #[default]
    Allow,
    Deny,
}

/// The `hook:` block of `PULSE.md` (plan 0025 G1).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HookConfig {
    #[serde(default)]
    pub unclaimed: UnclaimedPolicy,
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
    let config: PulseConfig = serde_yaml::from_str(&text).map_err(|error| {
        PulseError::kernel(
            "pulse_md_invalid",
            format!("PULSE.md is not valid YAML: {error}"),
            "PULSE.md must be YAML with a `profiles:` map and optional `fence_ignore:`",
        )
    })?;
    validate_panels(&config)?;
    Ok(config)
}

/// Reject a `panels:` entry that could never be satisfied (decision 0027
/// C1): a panel of one (that is the lane itself — two spellings for one
/// thing), a quorum outside `1..=count`, or a role the profile's own
/// `lanes` does not list. All three are authoring mistakes that would only
/// surface as a confusing `lane_seat_required`/`lane_seat_invalid` much
/// later, so they fail at load with the profile and role named.
fn validate_panels(config: &PulseConfig) -> Result<()> {
    for (key, profile) in &config.profiles {
        for (role, panel) in &profile.panels {
            let problem = if panel.count < 2 {
                Some(format!("count {} is below 2", panel.count))
            } else if panel.quorum < 1 || panel.quorum > panel.count {
                Some(format!(
                    "quorum {} is outside 1..={}",
                    panel.quorum, panel.count
                ))
            } else if !profile.lanes.iter().any(|lane| lane == role) {
                Some(format!("role {role} is not in this profile's lanes"))
            } else {
                None
            };
            if let Some(problem) = problem {
                return Err(PulseError::kernel(
                    "pulse_md_invalid",
                    format!("profile {key}: panel {role}: {problem}"),
                    "a panel is `<role>: {count: N, quorum: M}` with N >= 2, 1 <= M <= N, \
                     and <role> listed in the same profile's `lanes`",
                ));
            }
        }
    }
    Ok(())
}

/// The `fence_ignore` list from `PULSE.md`, or empty when the file is
/// missing or unreadable.
///
/// Every source-fence caller (`kernel::completion`, `kernel::lane`) needs
/// the same "read it, tolerate absence" behaviour; owning it here keeps one
/// copy of that decision instead of one per gate.
pub(crate) fn fence_ignore(repo_root: &Path) -> Vec<String> {
    load(repo_root)
        .map(|config| config.fence_ignore)
        .unwrap_or_default()
}

/// The fence of one record (plan 0025 B6): a record that declares
/// `touches` fences exactly its scope; a record without them (an old
/// ticket, a Story as a lane subject) fences the whole tree, exactly as
/// before decision 0025.
///
/// # Errors
/// Propagates the underlying snapshot's git failures.
pub(crate) fn fence_for(repo_root: &Path, record: &Value) -> Result<Source> {
    fence_for_touches(repo_root, &touches_of(record))
}

/// [`fence_for`] keyed directly on a `touches` list — for callers that
/// hold the list rather than the record (the lane seal, which resolves the
/// subject record inside its own lock).
///
/// # Errors
/// Propagates the underlying snapshot's git failures.
pub(crate) fn fence_for_touches(repo_root: &Path, touches: &[String]) -> Result<Source> {
    let ignore = fence_ignore(repo_root);
    if touches.is_empty() {
        source::snapshot(repo_root, &ignore)
    } else {
        source::scoped_snapshot(repo_root, touches, &ignore)
    }
}

/// Whether `a` and `b` describe the same fence state for `record` — the
/// ONE comparison rule (decision 0025 B6); every gate goes through here:
///
/// * a record with `touches` compares only the scope hash. HEAD moving
///   underneath is somebody else's commit landing, not this tree changing;
/// * a record without `touches` compares commit AND hash (the whole-tree
///   behavior that predates decision 0025).
///
/// The halves are `(commit, dirty_hash)` pairs so receipts
/// (`ReceiptSource`) and snapshots (`Source`) compare through one
/// function.
pub(crate) fn same_fence(record: &Value, a: (&str, &str), b: (&str, &str)) -> bool {
    same_fence_touches(&touches_of(record), a, b)
}

/// [`same_fence`] keyed directly on a `touches` list, mirroring
/// [`fence_for_touches`].
pub(crate) fn same_fence_touches(touches: &[String], a: (&str, &str), b: (&str, &str)) -> bool {
    let (a_commit, a_hash) = a;
    let (b_commit, b_hash) = b;
    if touches.is_empty() {
        a_commit == b_commit && a_hash == b_hash
    } else {
        a_hash == b_hash
    }
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

    #[test]
    fn a_scoped_record_compares_only_the_scope_hash() {
        // Plan 0025 B6: another ticket's commit landing (HEAD moved) does
        // not stale a scoped ticket; its own scope hash moving does.
        let record = serde_json::json!({"touches": ["src/**"]});
        assert!(same_fence(
            &record,
            ("commit-a", "scope:sha256:1"),
            ("commit-b", "scope:sha256:1")
        ));
        assert!(!same_fence(
            &record,
            ("commit-a", "scope:sha256:1"),
            ("commit-a", "scope:sha256:2")
        ));
    }

    #[test]
    fn a_record_without_touches_compares_commit_and_hash() {
        let record = serde_json::json!({});
        assert!(same_fence(
            &record,
            ("commit-a", "sha256:1"),
            ("commit-a", "sha256:1")
        ));
        assert!(!same_fence(
            &record,
            ("commit-a", "sha256:1"),
            ("commit-b", "sha256:1")
        ));
        assert!(!same_fence(
            &record,
            ("commit-a", "sha256:1"),
            ("commit-a", "sha256:2")
        ));
    }

    #[test]
    fn a_profile_without_panels_parses_exactly_as_before() {
        // Decision 0027 C1: `panels` is opt-in; absent means no panel.
        let repo = tempfile::tempdir().unwrap();
        write_pulse_md(
            repo.path(),
            "profiles:\n  cli-low: {lanes: [review-correctness]}\n",
        );
        let config = load(repo.path()).unwrap();
        let profile = profile_for(&config, "cli-low").unwrap();
        assert!(profile.panels.is_empty());
        assert!(profile.panel("review-correctness").is_none());
    }

    #[test]
    fn a_declared_panel_is_readable_by_role() {
        let repo = tempfile::tempdir().unwrap();
        write_pulse_md(
            repo.path(),
            "profiles:\n  api-high:\n    lanes: [review-correctness]\n    panels:\n      review-correctness: {count: 3, quorum: 2}\n",
        );
        let config = load(repo.path()).unwrap();
        let panel = profile_for(&config, "api-high")
            .unwrap()
            .panel("review-correctness")
            .expect("the panel was declared");
        assert_eq!(panel.count, 3);
        assert_eq!(panel.quorum, 2);
    }

    #[test]
    fn a_panel_whose_quorum_exceeds_its_count_is_refused() {
        let repo = tempfile::tempdir().unwrap();
        write_pulse_md(
            repo.path(),
            "profiles:\n  api-high:\n    lanes: [review-correctness]\n    panels:\n      review-correctness: {count: 2, quorum: 3}\n",
        );
        let err = load(repo.path()).unwrap_err();
        assert_eq!(err.code(), "pulse_md_invalid");
        assert!(err.to_string().contains("api-high"), "{}", err.to_string());
        assert!(err.hint().is_some());
    }

    #[test]
    fn a_panel_for_a_role_outside_the_profile_is_refused() {
        let repo = tempfile::tempdir().unwrap();
        write_pulse_md(
            repo.path(),
            "profiles:\n  api-high:\n    lanes: [review-correctness]\n    panels:\n      qa-api: {count: 3, quorum: 2}\n",
        );
        let err = load(repo.path()).unwrap_err();
        assert_eq!(err.code(), "pulse_md_invalid");
        assert!(err.to_string().contains("qa-api"), "{}", err.to_string());
    }

    #[test]
    fn a_panel_of_one_is_refused_as_a_second_spelling() {
        // `count: 1` says exactly what omitting the panel says; two
        // spellings for one thing is two ways to drift.
        let repo = tempfile::tempdir().unwrap();
        write_pulse_md(
            repo.path(),
            "profiles:\n  api-high:\n    lanes: [review-correctness]\n    panels:\n      review-correctness: {count: 1, quorum: 1}\n",
        );
        let err = load(repo.path()).unwrap_err();
        assert_eq!(err.code(), "pulse_md_invalid");
        assert!(err.to_string().contains("below 2"), "{}", err.to_string());
    }

    #[test]
    fn fence_for_picks_the_scope_or_the_whole_tree() {
        // No git repo here — assert only the routing through the error
        // surface (both legs hit git; a missing repo fails identically).
        let repo = tempfile::tempdir().unwrap();
        let scoped = fence_for(repo.path(), &serde_json::json!({"touches": ["src/**"]}));
        let whole = fence_for(repo.path(), &serde_json::json!({}));
        assert_eq!(scoped.unwrap_err().code(), "git_invocation_failed");
        assert_eq!(whole.unwrap_err().code(), "git_invocation_failed");
    }
}
