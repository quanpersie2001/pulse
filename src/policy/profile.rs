//! Verification profile parsing from the repository's `PULSE.md` (Decision
//! 0012 §5).
//!
//! `PULSE.md` owns the verification profiles, not Pulse. The
//! `# Verification Profiles` section holds one bullet per profile:
//!
//! ```text
//! - `migration`: run migration tests, reviewers: 2
//! - `docs-only`: inspect changed Markdown links
//! ```
//!
//! A bullet may declare `reviewers: <N>` anywhere in its text; it defaults
//! to 1. `reviewers` is the number of distinct actors — different from the
//! worker — that must each record a passed `verification` receipt on the
//! same handoff before close. It never means two models: which model plays
//! the second reviewer is `runners.json`'s business.
//!
//! There is no per-Ticket profile binding yet, so the effective requirement
//! is the strictest declared profile (max), defaulting to 1 when `PULSE.md`
//! is missing or declares none.

use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::{PulseError, PulseResult};

/// One declared verification profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationProfile {
    pub name: String,
    pub reviewers: u32,
}

static REVIEWERS: Lazy<Regex> = Lazy::new(|| Regex::new(r"reviewers:\s*([0-9]+)").unwrap());

/// Parse the `# Verification Profiles` section of the repository's
/// `PULSE.md`. A missing file or section yields no profiles; malformed
/// bullets (no profile name) are refused so a typo cannot silently lower
/// the assurance floor.
///
/// # Errors
///
/// Returns a typed validation error when the section contains a bullet
/// without a parsable profile name.
pub fn load_verification_profiles(repo_root: &Path) -> PulseResult<Vec<VerificationProfile>> {
    let path = repo_root.join("PULSE.md");
    let Ok(bytes) = std::fs::read(&path) else {
        return Ok(Vec::new());
    };
    let markdown = String::from_utf8(bytes)
        .map_err(|_| PulseError::validation("pulse_md_invalid", "PULSE.md must be UTF-8"))?;
    let mut profiles = Vec::new();
    let mut in_section = false;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            in_section = trimmed
                .trim_start_matches('#')
                .trim()
                .eq_ignore_ascii_case("Verification Profiles");
            continue;
        }
        if !in_section {
            continue;
        }
        let Some(bullet) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        else {
            continue;
        };
        let (name, text) = split_profile(bullet).ok_or_else(|| {
            PulseError::validation(
                "pulse_md_profile_invalid",
                format!("verification profile bullet has no profile name: {trimmed}"),
            )
        })?;
        let reviewers = REVIEWERS
            .captures(text)
            .and_then(|captures| captures[1].parse::<u32>().ok())
            .unwrap_or(1)
            .max(1);
        profiles.push(VerificationProfile { name, reviewers });
    }
    Ok(profiles)
}

/// The effective close-gate requirement: how many distinct actors must each
/// record a passed `verification` receipt on the current handoff. Strictest
/// declared profile wins; 1 when nothing is declared.
///
/// # Errors
///
/// Propagates [`load_verification_profiles`] errors.
pub fn reviewers_required(repo_root: &Path) -> PulseResult<u32> {
    Ok(load_verification_profiles(repo_root)?
        .into_iter()
        .map(|profile| profile.reviewers)
        .max()
        .unwrap_or(1))
}

fn split_profile(bullet: &str) -> Option<(String, &str)> {
    let bullet = bullet.trim();
    if let Some(rest) = bullet.strip_prefix('`') {
        let end = rest.find('`')?;
        let name = rest[..end].trim();
        if name.is_empty() {
            return None;
        }
        return Some((name.to_string(), rest[end + 1..].trim_start_matches(':')));
    }
    let (name, text) = bullet.split_once(':')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some((name.to_string(), text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reviewers_from_profile_bullets() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("PULSE.md"),
            "# Repository Intent\n\n- keep it pure\n\n# Verification Profiles\n\n- `module-change`: node scripts/verify.mjs\n- `migration`: reviewers: 2, rollback required\n* `security`: `npm audit` and reviewers: 3\n\n# Human Judgment Boundaries\n\n- rename needs human approval\n",
        )
        .unwrap();
        let profiles = load_verification_profiles(repo.path()).unwrap();
        assert_eq!(
            profiles,
            vec![
                VerificationProfile {
                    name: "module-change".to_string(),
                    reviewers: 1
                },
                VerificationProfile {
                    name: "migration".to_string(),
                    reviewers: 2
                },
                VerificationProfile {
                    name: "security".to_string(),
                    reviewers: 3
                },
            ]
        );
        // The strictest declared profile is the repository floor.
        assert_eq!(reviewers_required(repo.path()).unwrap(), 3);
    }

    #[test]
    fn missing_pulse_md_defaults_to_one_reviewer() {
        let repo = tempfile::tempdir().unwrap();
        assert!(load_verification_profiles(repo.path()).unwrap().is_empty());
        assert_eq!(reviewers_required(repo.path()).unwrap(), 1);
    }

    #[test]
    fn bullet_without_a_name_is_refused() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("PULSE.md"),
            "# Verification Profiles\n\n- : reviewers: 2\n",
        )
        .unwrap();
        let error = load_verification_profiles(repo.path()).unwrap_err();
        assert_eq!(error.code(), "pulse_md_profile_invalid");
    }
}
