//! `pulse metrics` CLI tests (plan 0025 E6): the report is JSON with every
//! key present (a board or script can rely on the shape), `--since` accepts
//! RFC3339 and dates and refuses anything else, and the numbers reflect
//! what the CLI actually recorded in a fresh repository.

use serde_json::Value;

#[allow(dead_code)]
#[path = "common/bin.rs"]
mod common_bin;
#[allow(dead_code)]
#[path = "common/fixture_repo.rs"]
mod common_fixture_repo;

use crate::common_fixture_repo::TestRepo;

const ACTOR: &str = "human:tester";

#[test]
fn metrics_json_carries_every_top_level_key() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--json"]);
    let created = repo.pulse_ok(&[
        "work",
        "new",
        "ticket",
        "Metrics seed",
        "--risk",
        "low",
        "--surface",
        "cli",
        "--actor",
        ACTOR,
        "--json",
    ]);
    let ticket = created["id"].as_str().unwrap().to_string();
    repo.pulse_ok(&[
        "note",
        &ticket,
        "a friction worth naming",
        "--friction",
        "--from",
        ACTOR,
        "--json",
    ]);

    let out = repo.pulse_ok(&["metrics", "--json"]);
    for key in [
        "tickets_done",
        "friction_per_ticket_done",
        "friction_unclassified",
        "rework_rate",
        "lane_verdicts",
        "lane_verdict_corrected",
        "verify_runs",
        "panel",
        "median_claim_to_done_minutes",
        "learnings",
        "not_derivable",
    ] {
        assert!(out.get(key).is_some(), "missing key {key}: {out}");
    }
    assert_eq!(out["friction_unclassified"], 1, "the friction is raw");
    assert_eq!(out["tickets_done"], 0);
    assert_eq!(
        out["lane_verdicts"],
        serde_json::json!({"pass": 0, "fail": 0, "inconclusive": 0})
    );
    assert_eq!(
        out["learnings"]["usage"],
        serde_json::json!({"helpful": 0, "not_needed": 0, "misleading": 0})
    );
    let not_derivable = out["not_derivable"].as_array().unwrap();
    assert!(
        not_derivable
            .iter()
            .any(|entry| entry["metric"] == "claim_conflicts"),
        "{not_derivable:?}"
    );
    for entry in not_derivable {
        assert!(
            entry["reason"].as_str().is_some_and(|r| !r.is_empty()),
            "every not_derivable entry carries its reason: {entry:?}"
        );
    }
}

#[test]
fn since_accepts_rfc3339_and_dates_and_refuses_other_shapes() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--json"]);

    // Both accepted shapes parse against a real event log (init emits none,
    // but the parser's output feeds the same compute call either way).
    let _ = repo.pulse_ok(&["metrics", "--since", "2020-01-01", "--json"]);
    let _ = repo.pulse_ok(&["metrics", "--since", "2020-01-01T00:00:00Z", "--json"]);

    let output = repo.pulse(&["metrics", "--since", "yesterday", "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "metrics_since_invalid");
}
