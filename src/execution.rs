//! Core-owned typed handoff and verification proof contracts.

use serde::{Deserialize, Serialize};

use crate::canonical_json::hash_serializable;
use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HandoffReceipt {
    pub schema_version: u32,
    pub handoff_id: String,
    pub idempotency_key_hash: String,
    pub ticket_id: String,
    pub active_revision: u64,
    pub verifying_revision: u64,
    pub lease_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub repository_id: String,
    pub source_commit: String,
    /// Worktree mutation identity bound at handoff time (tracked diff plus
    /// untracked manifest hash). The close gate rejects a Ticket whose current
    /// worktree identity no longer matches this value.
    pub source_dirty_hash: String,
    pub summary: String,
    #[serde(default)]
    pub changed_paths: Vec<String>,
    #[serde(default)]
    pub evidence_receipt_ids: Vec<String>,
    /// Worker-claimed verification checks (`name`, `command`, `exit_code`).
    /// Claims, not evidence: the reviewer re-runs each command instead of
    /// trusting the recorded exit code.
    ///
    /// Evolution note: omitted from canonical form when empty so receipts
    /// sealed before this field existed keep validating (the fingerprint is
    /// canonical-content hash; an absent field and an empty list must stay
    /// indistinguishable forever).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<VerificationCheck>,
    /// Worker-claimed acceptance coverage. Missing proofs for some acceptance
    /// ids are allowed here; the close gate still demands full coverage from
    /// the reviewer's verification.
    ///
    /// Evolution note: omitted from canonical form when empty (see `checks`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance_proofs: Vec<AcceptanceProof>,
    /// Usage feedback for packet-injected learnings (empty for receipts
    /// recorded before the field existed).
    ///
    /// Evolution note: omitted from canonical form when empty (see `checks`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knowledge_usage: Vec<KnowledgeUsage>,
    /// Harness friction reported by the worker at handoff (Decision 0009 §4).
    /// The close gate turns each entry into a learning `candidate` with scope
    /// `harness`, exactly as it does for `--kind friction` notes.
    ///
    /// Friction rides in the receipt rather than the event log because the
    /// handoff already holds the repository write guard: recording a note
    /// would re-enter it through `show_node` and deadlock. The receipt also
    /// makes friction atomic with the handoff and idempotent under replay.
    ///
    /// Evolution note: omitted from canonical form when empty (see `checks`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frictions: Vec<String>,
    pub recorded_by: String,
    pub recorded_at: String,
    pub handoff_fingerprint: String,
}

impl HandoffReceipt {
    pub fn compute_fingerprint(&self) -> Result<String> {
        let mut projection = self.clone();
        projection.handoff_fingerprint.clear();
        hash_serializable(&projection)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationDisposition {
    Passed,
    Rework,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VerificationCheck {
    pub name: String,
    pub command: String,
    pub exit_code: i32,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
}

/// Maps one contract acceptance item to passing verification checks and
/// optional immutable evidence receipts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceProof {
    pub acceptance_id: String,
    #[serde(default)]
    pub check_names: Vec<String>,
    #[serde(default)]
    pub evidence_receipt_ids: Vec<String>,
}

/// Severity a reviewer assigns to one finding. Fixed vocabulary; severity is
/// never averaged, merged or turned into a score anywhere in Pulse.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    High,
    Medium,
    Low,
}

/// One shaped finding (Decision 0012 §3, PRODUCT §5.3). `summary` and
/// `owner` (repository-relative path or `DOC-ID#section`) are mandatory.
/// `check` is the command the reviewer ran and saw fail, or a receipt id;
/// a finding without one is recorded with `unverifiable: true` and a
/// disposition `rework` backed only by unverifiable findings is not a
/// verdict. Counts, filenames, file age and severity are never findings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub summary: String,
    pub owner: String,
    #[serde(default)]
    pub check: Option<String>,
    pub severity: FindingSeverity,
    /// Reviewer findings map to a contract acceptance item; QA findings
    /// carry `case_id` instead.
    #[serde(default)]
    pub acceptance_id: Option<String>,
    #[serde(default)]
    pub case_id: Option<String>,
    pub unverifiable: bool,
}

impl Finding {
    /// Normalize text fields in place and derive `unverifiable` from the
    /// presence of a non-empty `check`.
    pub fn normalize(&mut self) {
        self.summary = self.summary.trim().to_string();
        self.owner = self.owner.trim().to_string();
        self.check = self
            .check
            .as_deref()
            .map(str::trim)
            .filter(|check| !check.is_empty())
            .map(str::to_string);
        if let Some(acceptance_id) = self.acceptance_id.as_deref() {
            self.acceptance_id = Some(acceptance_id.trim().to_string());
        }
        if let Some(case_id) = self.case_id.as_deref() {
            self.case_id = Some(case_id.trim().to_string());
        }
        self.unverifiable = self.check.is_none();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VerificationReceipt {
    pub schema_version: u32,
    pub verification_id: String,
    pub idempotency_key_hash: String,
    pub handoff_id: String,
    pub ticket_id: String,
    pub lease_id: String,
    pub source_commit: String,
    /// Worktree mutation identity observed at verification time; must match
    /// the handoff binding or the proof is stale.
    pub source_dirty_hash: String,
    pub disposition: VerificationDisposition,
    pub summary: String,
    pub checks: Vec<VerificationCheck>,
    /// Worker-claimed acceptance coverage. Missing proofs for some acceptance
    /// ids are allowed here; the close gate still demands full coverage from
    /// the reviewer's verification.
    ///
    /// Evolution note: omitted from canonical form when empty so receipts
    /// sealed before this field existed keep validating.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance_proofs: Vec<AcceptanceProof>,
    /// Shaped findings; a `rework` disposition must carry at least one
    /// verifiable finding (`check` present) to count as a verdict.
    ///
    /// Evolution note: omitted from canonical form when empty so receipts
    /// sealed before this field existed keep validating.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    pub verified_by: String,
    pub recorded_at: String,
    pub resulting_status: String,
    pub resulting_revision: u64,
    pub verification_fingerprint: String,
}

impl VerificationReceipt {
    pub fn compute_fingerprint(&self) -> Result<String> {
        let mut projection = self.clone();
        projection.verification_fingerprint.clear();
        hash_serializable(&projection)
    }
}

#[derive(Debug, Clone)]
pub struct SubmitHandoffArgs {
    pub lease_id: String,
    pub actor: String,
    pub session_id: String,
    pub source_commit: String,
    pub summary: String,
    pub changed_paths: Vec<String>,
    pub evidence_receipt_ids: Vec<String>,
    /// Worker-claimed checks, same shape as the reviewer's verification.
    pub checks: Vec<VerificationCheck>,
    /// Worker-claimed acceptance coverage, same shape as the reviewer's.
    pub acceptance_proofs: Vec<AcceptanceProof>,
    /// How the worker used the learnings injected into its packet.
    pub learning_usage: Vec<KnowledgeUsageClaim>,
    /// Harness friction the worker hit while running this Ticket.
    pub frictions: Vec<String>,
    pub idempotency_key: String,
}

/// One worker-reported learning usage from the CLI: `LRN-001=helpful`.
#[derive(Debug, Clone)]
pub struct KnowledgeUsageClaim {
    pub learning_id: String,
    pub outcome: KnowledgeUsageOutcome,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeUsageOutcome {
    Helpful,
    NotNeeded,
    Misleading,
}

/// Recorded usage feedback for one learning, bound into the handoff receipt.
/// `injected` is computed by Pulse from the committed packet; `applied` is
/// derived from the outcome (helpful implies applied).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeUsage {
    pub learning_id: String,
    pub injected: bool,
    pub applied: bool,
    pub outcome: KnowledgeUsageOutcome,
}

#[derive(Debug, Clone)]
pub struct CompleteVerificationArgs {
    pub handoff_id: String,
    pub actor: String,
    pub source_commit: String,
    pub disposition: VerificationDisposition,
    pub summary: String,
    pub checks: Vec<VerificationCheck>,
    pub acceptance_proofs: Vec<AcceptanceProof>,
    /// Shaped findings accompanying the verdict.
    pub findings: Vec<Finding>,
    pub idempotency_key: String,
}

/// Immutable result of the Core-owned proof close gate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CloseReceipt {
    pub schema_version: u32,
    pub close_id: String,
    pub idempotency_key_hash: String,
    pub verification_id: String,
    pub handoff_id: String,
    pub ticket_id: String,
    pub lease_id: String,
    pub source_commit: String,
    pub summary: String,
    pub closed_by: String,
    pub recorded_at: String,
    pub resulting_revision: u64,
    pub close_fingerprint: String,
}

impl CloseReceipt {
    pub fn compute_fingerprint(&self) -> Result<String> {
        let mut projection = self.clone();
        projection.close_fingerprint.clear();
        hash_serializable(&projection)
    }
}

#[derive(Debug, Clone)]
pub struct CloseTicketArgs {
    pub verification_id: String,
    pub actor: String,
    pub source_commit: String,
    pub summary: String,
    pub idempotency_key: String,
}

/// Immutable result of the Core-owned Story close gate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoryCloseReceipt {
    pub schema_version: u32,
    pub close_id: String,
    pub idempotency_key_hash: String,
    pub story_id: String,
    pub qualification_receipt_ids: Vec<String>,
    pub source_commit: String,
    pub graph_fingerprint_observed: String,
    pub done_ticket_ids: Vec<String>,
    pub superseded_ticket_ids: Vec<String>,
    pub summary: String,
    pub closed_by: String,
    pub recorded_at: String,
    pub resulting_revision: u64,
    pub close_fingerprint: String,
}

impl StoryCloseReceipt {
    pub fn compute_fingerprint(&self) -> Result<String> {
        let mut projection = self.clone();
        projection.close_fingerprint.clear();
        hash_serializable(&projection)
    }
}

#[derive(Debug, Clone)]
pub struct CloseStoryArgs {
    pub story_id: String,
    pub qualification_receipt_ids: Vec<String>,
    pub actor: String,
    pub source_commit: String,
    pub summary: String,
    pub idempotency_key: String,
}

/// Upper bound for the one-line handoff summary (Decision 0012: the AC
/// mapping and check results live in `checks[]`/`acceptance_proofs[]`, not
/// in prose).
pub const MAX_HANDOFF_SUMMARY_CHARS: usize = 300;

/// Validate the worker's machine-readable handoff claim.
///
/// Exit codes are claims — the reviewer re-runs each command — so they are
/// not judged here, and missing proofs for some acceptance ids are allowed
/// (the close gate demands coverage from the reviewer, not the worker).
/// What must hold: check names and commands are non-empty, check names and
/// acceptance ids are unique, and every proof references a declared check.
pub fn validate_handoff_claim(
    checks: &[VerificationCheck],
    acceptance_proofs: &[AcceptanceProof],
) -> Result<()> {
    let mut names = std::collections::BTreeSet::new();
    for check in checks {
        if check.name.trim().is_empty() || check.command.trim().is_empty() {
            return Err(PulseError::validation(
                "verification_check_invalid",
                "handoff check name and command must not be empty",
            ));
        }
        if !names.insert(check.name.trim().to_string()) {
            return Err(PulseError::validation(
                "verification_check_duplicate",
                format!("handoff check names must be unique: {}", check.name.trim()),
            ));
        }
    }
    let mut acceptance_ids = std::collections::BTreeSet::new();
    for proof in acceptance_proofs {
        let id = proof.acceptance_id.trim();
        if id.is_empty() {
            return Err(PulseError::validation(
                "handoff_acceptance_proof_invalid",
                "handoff acceptance proof must name an acceptance id",
            ));
        }
        if !acceptance_ids.insert(id.to_string()) {
            return Err(PulseError::validation(
                "handoff_acceptance_proof_duplicate",
                format!("handoff acceptance proofs must be unique: {id}"),
            ));
        }
        for check_name in &proof.check_names {
            if !names.contains(check_name.trim()) {
                return Err(PulseError::validation(
                    "verification_acceptance_check_missing",
                    format!("acceptance {id} references unknown check {check_name}"),
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_checks(
    disposition: VerificationDisposition,
    checks: &[VerificationCheck],
) -> Result<()> {
    if checks.is_empty() {
        return Err(PulseError::validation(
            "verification_checks_missing",
            "proof-driven completion requires at least one verification check",
        ));
    }
    for check in checks {
        if check.name.trim().is_empty() || check.command.trim().is_empty() {
            return Err(PulseError::validation(
                "verification_check_invalid",
                "verification check name and command must not be empty",
            ));
        }
    }
    if disposition == VerificationDisposition::Passed
        && checks.iter().any(|check| check.exit_code != 0)
    {
        return Err(PulseError::validation(
            "verification_not_passed",
            "passed disposition requires every verification check to exit zero",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod fingerprint_stability_tests {
    use super::*;
    use crate::canonical_json::{to_canonical_bytes, to_canonical_value_from};

    fn legacy_handoff_json() -> serde_json::Value {
        // Field set of a handoff receipt sealed before `checks`,
        // `acceptance_proofs` and `knowledge_usage` existed (golden-path
        // era). `evidence_receipt_ids` predates them and stays explicit.
        serde_json::json!({
            "schema_version": 1,
            "handoff_id": "handoff_legacy",
            "idempotency_key_hash": "sha256:aaa",
            "ticket_id": "TK-001",
            "active_revision": 9,
            "verifying_revision": 10,
            "lease_id": "lease_1",
            "project_id": "proj_1",
            "workspace_id": "ws_1",
            "session_id": "sess_1",
            "repository_id": "repo_1",
            "source_commit": "d4e5f6",
            "source_dirty_hash": "sha256:bbb",
            "summary": "did the thing",
            "changed_paths": ["src/x.mjs"],
            "evidence_receipt_ids": [],
            "recorded_by": "agent:runner:worker",
            "recorded_at": "2026-09-05T00:00:00Z",
            "handoff_fingerprint": ""
        })
    }

    fn legacy_verification_json() -> serde_json::Value {
        // Field set of a verification receipt sealed before `findings`
        // existed; `acceptance_proofs` already carried proofs.
        serde_json::json!({
            "schema_version": 1,
            "verification_id": "verify_legacy",
            "idempotency_key_hash": "sha256:ccc",
            "handoff_id": "handoff_legacy",
            "ticket_id": "TK-001",
            "lease_id": "lease_1",
            "source_commit": "d4e5f6",
            "source_dirty_hash": "sha256:bbb",
            "disposition": "passed",
            "summary": "checked the thing",
            "checks": [{"name": "verify", "command": "node scripts/verify.mjs", "exit_code": 0}],
            "acceptance_proofs": [{"acceptance_id": "AC-1", "check_names": ["verify"]}],
            "verified_by": "agent:runner:reviewer",
            "recorded_at": "2026-09-05T00:00:00Z",
            "resulting_status": "verifying",
            "resulting_revision": 10,
            "verification_fingerprint": ""
        })
    }

    #[test]
    fn handoff_without_evolution_fields_seals_and_reloads() {
        let mut value = legacy_handoff_json();
        let receipt: HandoffReceipt = serde_json::from_value(value.clone()).unwrap();
        let fingerprint = receipt.compute_fingerprint().unwrap();
        value["handoff_fingerprint"] = serde_json::Value::String(fingerprint.clone());
        let bytes = to_canonical_bytes(&value).unwrap();
        let reloaded: HandoffReceipt = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reloaded.compute_fingerprint().unwrap(), fingerprint);
    }

    #[test]
    fn verification_without_findings_seals_and_reloads() {
        let mut value = legacy_verification_json();
        let receipt: VerificationReceipt = serde_json::from_value(value.clone()).unwrap();
        let fingerprint = receipt.compute_fingerprint().unwrap();
        value["verification_fingerprint"] = serde_json::Value::String(fingerprint.clone());
        let bytes = to_canonical_bytes(&value).unwrap();
        let reloaded: VerificationReceipt = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reloaded.compute_fingerprint().unwrap(), fingerprint);
    }

    #[test]
    fn empty_evolution_collections_are_omitted_from_canonical_form() {
        let receipt: HandoffReceipt = serde_json::from_value(legacy_handoff_json()).unwrap();
        let value = to_canonical_value_from(&receipt).unwrap();
        assert!(value.get("checks").is_none());
        assert!(value.get("acceptance_proofs").is_none());
        assert!(value.get("knowledge_usage").is_none());
        // Fields from before the evolution convention keep explicit empties.
        assert_eq!(
            value.get("evidence_receipt_ids"),
            Some(&serde_json::Value::Array(vec![]))
        );

        let verification: VerificationReceipt =
            serde_json::from_value(legacy_verification_json()).unwrap();
        let value = to_canonical_value_from(&verification).unwrap();
        assert!(value.get("findings").is_none());
        assert!(value.get("acceptance_proofs").is_some());
    }

    #[test]
    fn non_empty_evolution_collections_stay_in_canonical_form() {
        let mut value = legacy_handoff_json();
        value["checks"] = serde_json::json!([
            {"name": "verify", "command": "node scripts/verify.mjs", "exit_code": 0}
        ]);
        let receipt: HandoffReceipt = serde_json::from_value(value).unwrap();
        let canonical = to_canonical_value_from(&receipt).unwrap();
        assert!(canonical.get("checks").is_some());
        assert!(canonical.get("knowledge_usage").is_none());
    }
}
