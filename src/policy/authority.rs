use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::identity::actor::{ActorKind, ActorRef};
use crate::{PulseError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_PRINCIPALS: usize = 256;
const MAX_GRANTS: usize = 128;

/// Every authority grant used by the current Core implementation.
///
/// This is deliberately an explicit, sorted list. Grants are capabilities,
/// not roles, so initialization must never use a wildcard or infer authority
/// from an actor kind. Keep this list in sync with every grant passed to
/// [`authorize`] (including the materialization-specific shaping grants).
pub const CORE_GRANTS: &[&str] = &[
    "decision.accept",
    "decision.propose",
    "docs.write",
    "documentation.defer",
    "evidence.record",
    "knowledge.capture",
    "note",
    "qa.defer_to_story_close",
    "qa.none.approve",
    "shape.apply",
    "shape.approve.R0",
    "shape.approve.R1",
    "shape.approve.R2",
    "shape.approve.R3",
    "shape.destination.redraw",
    "shape.invalidate",
    "work.assignment.close",
    "work.assignment.handoff",
    "work.assignment.prepare",
    "work.assignment.release",
    "work.assignment.verify",
    "work.close",
    "work.edge.create",
    "work.materialization.downgrade",
    "work.node.create",
    "work.node.update",
    "work.story.close",
    "work.transition.ready",
    "work.transition.shaped",
];

/// Return whether `grant` is part of the closed Core grant vocabulary.
pub fn is_core_grant(grant: &str) -> bool {
    CORE_GRANTS.binary_search(&grant).is_ok()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorityPolicy {
    pub schema_version: u32,
    pub revision: u64,
    pub principals: Vec<AuthorityPrincipal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorityPrincipal {
    pub kind: ActorKind,
    pub id: String,
    pub grants: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrincipalRef {
    pub kind: ActorKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorityPolicyReport {
    pub schema_version: u32,
    pub code: String,
    pub available: bool,
    pub valid: bool,
    pub policy_revision: Option<u64>,
    pub fingerprint: Option<String>,
    pub principals: Vec<AuthorityPrincipal>,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorityBootstrapOutcome {
    pub created: Vec<PathBuf>,
    pub preserved: Vec<PathBuf>,
    pub policy: AuthorityPolicy,
    pub changed: bool,
}

impl AuthorityPolicy {
    pub fn normalize(&mut self) {
        self.principals.sort_by(|a, b| {
            principal_kind_name(&a.kind)
                .cmp(principal_kind_name(&b.kind))
                .then(a.id.cmp(&b.id))
        });
        for principal in &mut self.principals {
            principal.grants.sort();
            principal.grants.dedup();
        }
    }

    pub fn fingerprint(&self) -> Result<String> {
        let mut normalized = self.clone();
        normalized.normalize();
        Ok(hash_bytes(&to_canonical_bytes(&normalized)?))
    }

    pub fn has_grant(&self, actor: &ActorRef, grant: &str) -> bool {
        self.principals.iter().any(|principal| {
            principal.kind == actor.kind
                && principal.id == actor.id
                && principal.grants.iter().any(|candidate| candidate == grant)
        })
    }

    pub fn validate(&self) -> Vec<String> {
        let mut codes = Vec::new();
        if self.schema_version != 1 {
            codes.push("readiness_policy_invalid".to_string());
        }
        if self.revision == 0 {
            codes.push("readiness_policy_invalid".to_string());
        }
        if self.principals.len() > MAX_PRINCIPALS {
            codes.push("readiness_policy_invalid".to_string());
        }
        let mut seen_principals = BTreeSet::new();
        for principal in &self.principals {
            let key = (principal_kind_name(&principal.kind), principal.id.as_str());
            if principal.id.trim().is_empty() || principal.id.len() > 128 {
                codes.push("readiness_policy_invalid".to_string());
            }
            if !seen_principals.insert(key) {
                codes.push("readiness_policy_invalid".to_string());
            }
            if principal.grants.len() > MAX_GRANTS {
                codes.push("readiness_policy_invalid".to_string());
            }
            let mut grants = BTreeSet::new();
            for grant in &principal.grants {
                if !is_valid_grant(grant) || grant.contains('*') {
                    codes.push("readiness_policy_invalid".to_string());
                }
                if !grants.insert(grant.as_str()) {
                    codes.push("readiness_policy_invalid".to_string());
                }
            }
        }
        codes.sort();
        codes.dedup();
        codes
    }
}

pub fn load_authority_policy(repo_root: &Path) -> Result<AuthorityPolicyReport> {
    let path = authority_path(repo_root);
    if !path.exists() {
        return Ok(AuthorityPolicyReport {
            schema_version: 1,
            code: "readiness_policy_missing".to_string(),
            available: false,
            valid: false,
            policy_revision: None,
            fingerprint: None,
            principals: vec![],
            reason_codes: vec!["readiness_policy_missing".to_string()],
        });
    }

    let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
    let mut policy: AuthorityPolicy =
        serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
    policy.normalize();
    let mut codes = policy.validate();
    let canonical = to_canonical_bytes(&policy)?;
    if bytes != canonical {
        codes.push("readiness_policy_not_canonical".to_string());
    }
    codes.sort();
    codes.dedup();
    let fingerprint = hash_bytes(&canonical);
    Ok(AuthorityPolicyReport {
        schema_version: 1,
        code: if codes.is_empty() {
            "ok"
        } else {
            "readiness_policy_invalid"
        }
        .to_string(),
        available: true,
        valid: codes.is_empty(),
        policy_revision: Some(policy.revision),
        fingerprint: Some(fingerprint),
        principals: policy.principals,
        reason_codes: codes,
    })
}

pub fn validate_authority_policy_file(repo_root: &Path) -> Result<AuthorityPolicyReport> {
    load_authority_policy(repo_root)
}

/// Validate an existing authority policy without creating a permissive default.
///
/// Missing policy state is safe for repository initialization. Existing state
/// must already be canonical and valid so initialization never rewrites a
/// maintainer-owned authority decision.
pub(crate) fn preflight_bootstrap(repo_root: &Path) -> Result<()> {
    let report = load_authority_policy(repo_root)?;
    if !report.available {
        return Ok(());
    }
    if !report.valid {
        return Err(PulseError::validation(
            "repository_init_authority_invalid",
            "existing authority policy is invalid; refusing repository initialization without overwrite",
        ));
    }
    Ok(())
}

/// Install a canonical default-deny authority policy when none exists.
///
/// # Errors
///
/// Returns a typed validation or I/O error when existing authority state is
/// invalid or the new policy cannot be created durably.
pub(crate) fn bootstrap_default_deny(
    repo_root: &Path,
    principal: &AuthorityPrincipal,
) -> Result<AuthorityBootstrapOutcome> {
    preflight_bootstrap(repo_root)?;
    let directory = repo_root.join(".pulse/policy");
    let path = authority_path(repo_root);
    let mut created = Vec::new();
    let mut preserved = Vec::new();

    if directory.exists() {
        preserved.push(directory.clone());
    } else {
        fs::create_dir_all(&directory).map_err(|error| PulseError::io(&directory, error))?;
        created.push(directory);
    }

    let (policy, changed) = if path.exists() {
        preserved.push(path.clone());
        let report = load_authority_policy(repo_root)?;
        let revision = report.policy_revision.ok_or_else(|| {
            PulseError::validation(
                "repository_init_authority_invalid",
                "existing authority policy has no revision",
            )
        })?;
        let mut policy = AuthorityPolicy {
            schema_version: 1,
            revision,
            principals: report.principals,
        };
        // Existing authority is maintainer-owned. Preserve an existing
        // principal and its grants byte-for-byte; only enroll a requested
        // actor when no matching principal exists.
        let has_principal = policy
            .principals
            .iter()
            .any(|candidate| candidate.kind == principal.kind && candidate.id == principal.id);
        if has_principal {
            (policy, false)
        } else {
            policy.principals.push(principal.clone());
            policy.revision = policy.revision.checked_add(1).ok_or_else(|| {
                PulseError::validation(
                    "repository_init_authority_invalid",
                    "authority policy revision overflow",
                )
            })?;
            policy.normalize();
            crate::storage::atomic_write(&path, &to_canonical_bytes(&policy)?)?;
            (policy, true)
        }
    } else {
        let policy = AuthorityPolicy {
            schema_version: 1,
            revision: 1,
            principals: vec![principal.clone()],
        };
        crate::storage::create_new(&path, &to_canonical_bytes(&policy)?)?;
        created.push(path);
        (policy, true)
    };

    Ok(AuthorityBootstrapOutcome {
        created,
        preserved,
        policy,
        changed,
    })
}

/// Parse a `kind:id` actor string into a typed evidence `ActorRef`.
///
/// Actors are declared identity, never authority. A missing kind defaults to
/// `system` so an unqualified id cannot accidentally impersonate a human
/// principal recorded in the authority policy.
pub fn parse_actor(actor: impl AsRef<str>) -> ActorRef {
    let actor = actor.as_ref();
    let (kind, id) = actor
        .split_once(':')
        .map_or(("system", actor), |(kind, id)| (kind, id));
    let kind = match kind {
        "human" => ActorKind::Human,
        "agent" => ActorKind::Agent,
        _ => ActorKind::System,
    };
    ActorRef {
        kind,
        id: id.to_string(),
    }
}

/// Authorize an operation against the loaded authority policy.
///
/// Authority is default-deny: a missing or invalid policy cannot authorize any
/// operation that requires a grant, and no implicit `human:*` superuser exists.
/// A principal must own every kernel-derived grant for the operation to pass.
pub fn authorize(
    report: &AuthorityPolicyReport,
    actor: &ActorRef,
    required_grants: &[&str],
) -> crate::Result<()> {
    if !report.available {
        return Err(PulseError::validation(
            "readiness_policy_missing",
            "authority policy is missing; cannot authorize gated operation",
        ));
    }
    if !report.valid {
        return Err(PulseError::validation(
            "readiness_policy_invalid",
            "authority policy is invalid; cannot authorize gated operation",
        ));
    }
    for grant in required_grants {
        let held = report.principals.iter().any(|principal| {
            principal.kind == actor.kind
                && principal.id == actor.id
                && principal.grants.iter().any(|candidate| candidate == grant)
        });
        if !held {
            return Err(PulseError::validation(
                "readiness_authority_denied",
                format!(
                    "actor {}:{} lacks required grant {grant}",
                    principal_kind_name(&actor.kind),
                    actor.id
                ),
            ));
        }
    }
    Ok(())
}

pub fn authority_path(repo_root: &Path) -> std::path::PathBuf {
    repo_root.join(".pulse/policy/authority.json")
}

fn principal_kind_name(kind: &ActorKind) -> &'static str {
    match kind {
        ActorKind::Human => "human",
        ActorKind::Agent => "agent",
        ActorKind::System => "system",
    }
}

fn is_valid_grant(grant: &str) -> bool {
    let len = grant.len();
    (3..=96).contains(&len)
        && grant.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}
