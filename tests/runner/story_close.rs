//! Story-close qualification runs: `pulse run qa --scope story_close`.
//!
//! Covers the run-side contract only: the committed input carries the full
//! applicable Story baseline plus scope and baseline bindings, the run event
//! targets the Story, and scope misuse is refused. Receipt-content coverage
//! lives in the story-close gate tests.

use std::fs;

use serde_json::Value;

// The assignment fixture resolves its siblings through `super::`; re-export
// them into this module's scope so the shared file compiles unchanged.
#[allow(dead_code)]
pub(crate) use crate::common_fixture_repo;
pub(crate) use crate::common_git;

#[allow(unused_imports, dead_code)]
#[path = "../graph/assignment_fixture.rs"]
mod assignment_fixture;

use self::assignment_fixture::setup_ready_ticket_with_required_qa;
use crate::cli_run::{set_command, ACTOR};
use crate::common_fixture_repo::TestRepo;
use crate::common_git::commit_all;

use pulse::JsonGraphStore;

fn story_fixture(repo: &TestRepo) -> (JsonGraphStore, String, String) {
    repo.pulse_ok(&["init", "--actor", ACTOR, "--json"]);
    let store = JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket_with_required_qa(repo.path(), &store);
    let node = store.show_node(&ticket_id).unwrap();
    let story_id = node
        .qa
        .as_ref()
        .and_then(|qa| qa.impact.behavioral_owner.clone())
        .expect("required-QA fixture ticket must have a behavioral owner Story");
    (store, story_id, ticket_id)
}

fn install_echoing_qa(repo: &TestRepo) {
    let script = r#"#!/bin/sh
set -e
RUN_DIR="$(dirname "$1")"
mkdir -p "$RUN_DIR/artifacts"
cp "$1" "$RUN_DIR/artifacts/input-copy.json"
echo '{"cases": []}'
"#;
    fs::create_dir_all(repo.path().join("scripts")).unwrap();
    fs::write(repo.path().join("scripts/fake-qa.sh"), script).unwrap();
    set_command(repo, "qa", "sh scripts/fake-qa.sh {input}");
}

fn error_code(repo: &TestRepo, args: &[&str]) -> String {
    let output = repo.pulse(args);
    assert!(
        !output.status.success(),
        "expected failure: stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    err["code"].as_str().unwrap().to_string()
}

fn committed_input(repo: &TestRepo, story_id: &str) -> Value {
    serde_json::from_slice(
        &fs::read(
            repo.path()
                .join(".pulse/runtime/run")
                .join(story_id)
                .join("artifacts/input-copy.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn story_close_qa_run_commits_full_baseline_input() {
    let repo = TestRepo::from_fixture("minimal-service");
    let (_store, story_id, ticket_id) = story_fixture(&repo);
    install_echoing_qa(&repo);

    let out = repo.pulse_ok(&[
        "run",
        "qa",
        "--scope",
        "story_close",
        "--story",
        &story_id,
        "--ticket",
        &ticket_id,
        "--json",
    ]);
    assert_eq!(out["status"], "completed");
    assert_eq!(out["ticket_id"], ticket_id);
    assert_eq!(out["code"], "run_completed");

    // The input the executor saw is the story-close qualification contract:
    // story scope, every applicable baseline case, baseline bindings.
    let input = committed_input(&repo, &story_id);
    assert_eq!(input["qa_scope"], "story_close");
    assert_eq!(input["story_id"], story_id);
    assert_eq!(input["ticket_id"], ticket_id);
    assert_eq!(input["qa_posture"], "required");
    assert_eq!(input["cases"][0]["id"], "QA-001");
    assert_eq!(input["cases"][0]["revision"], 1);
    // Cases travel verbatim so the executor never resolves the baseline.
    assert!(!input["cases"][0]["intent"].as_str().unwrap().is_empty());
    assert!(!input["cases"][0]["steps"].as_array().unwrap().is_empty());
    assert!(!input["cases"][0]["expected"].as_array().unwrap().is_empty());
    assert_eq!(input["cases"][0]["surface"], "api");
    assert_eq!(input["cases"].as_array().unwrap().len(), 1);
    assert_eq!(input["baseline_revision"], 1);
    assert!(input["baseline_content_hash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
}

#[test]
fn story_close_scope_is_qa_only_and_requires_a_story() {
    let repo = TestRepo::from_fixture("minimal-service");
    let (_store, story_id, _ticket_id) = story_fixture(&repo);
    install_echoing_qa(&repo);

    assert_eq!(
        error_code(
            &repo,
            &[
                "run",
                "worker",
                "--scope",
                "story_close",
                "--story",
                &story_id,
                "--ticket",
                "TK-Whatever0000000000000000000",
                "--json"
            ]
        ),
        "run_scope_role_invalid"
    );
    assert_eq!(
        error_code(&repo, &["run", "qa", "--scope", "story_close", "--json"]),
        "run_story_required"
    );
    // An unknown Story fails on baseline resolution.
    assert_eq!(
        error_code(
            &repo,
            &[
                "run",
                "qa",
                "--scope",
                "story_close",
                "--story",
                "ST-Missing000000000000000000",
                "--json"
            ]
        ),
        "qa_behavioral_owner_missing"
    );
}

#[test]
fn story_close_refuses_a_ticket_outside_the_story() {
    let repo = TestRepo::from_fixture("minimal-service");
    let (_store, story_id, _ticket_id) = story_fixture(&repo);
    install_echoing_qa(&repo);

    // A storyless ticket is not a legitimate initiating Ticket.
    let outsider = crate::cli_run::setup_ready_ticket(&repo);
    assert_eq!(
        error_code(
            &repo,
            &[
                "run",
                "qa",
                "--scope",
                "story_close",
                "--story",
                &story_id,
                "--ticket",
                &outsider,
                "--json"
            ]
        ),
        "run_story_ticket_mismatch"
    );
}

#[test]
fn story_close_without_any_done_ticket_is_refused() {
    let repo = TestRepo::from_fixture("minimal-service");
    let (_store, story_id, _ticket_id) = story_fixture(&repo);
    install_echoing_qa(&repo);
    commit_all(repo.path());

    // Auto-selection scans done Tickets owned by the Story; the fixture
    // Ticket is still ready, so there is nothing to carry qualification.
    let output = repo.pulse(&[
        "run",
        "qa",
        "--scope",
        "story_close",
        "--story",
        &story_id,
        "--json",
    ]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "run_story_ticket_missing");
}
