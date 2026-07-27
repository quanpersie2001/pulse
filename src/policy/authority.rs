use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::evidence::model::{ActorKind, ActorRef};
use crate::{PulseError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const MAX_PRINCIPALS: usize = 256;
const MAX_GRANTS: usize = 128;

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
