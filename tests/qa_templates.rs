//! The embedded QA lane templates (`templates/qa/{api,ui}.mjs`) are code
//! Pulse ships into target repos, and the 0025 dogfood found both of them
//! failing their lane in the worst way: a step carrying an unresolved
//! `<…>` placeholder went over the wire verbatim and graded the product
//! `fail` (F12), and a non-URL `steps[0]` crashed the whole ui lane minutes
//! into app startup with no evidence file at all (F13). These tests run
//! the actual template files with `node` against synthetic lane inputs and
//! assert the refusal/crash-containment behavior — before any app start,
//! so they are hermetic (no run.md, no app, no playwright).
//!
//! `node` is not a build dependency of Pulse; the templates only ever run
//! inside a target repo that already needs it. When `node` is missing the
//! suite says so and passes — the templates are inert data there.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

const API_TEMPLATE: &str = include_str!("../templates/qa/api.mjs");
const UI_TEMPLATE: &str = include_str!("../templates/qa/ui.mjs");

fn node_missing() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|output| !output.status.success())
        .unwrap_or(true)
}

/// Lay down `<repo>/scripts/qa/<name>` with the template body, write the
/// lane input, and run the template the way a target repo does.
fn run_template(repo: &Path, name: &str, body: &str, input: &Value) -> Output {
    let script_dir = repo.join("scripts/qa");
    fs::create_dir_all(&script_dir).unwrap();
    fs::write(script_dir.join(name), body).unwrap();
    let input_path = repo.join("lane-input.json");
    fs::write(&input_path, serde_json::to_vec(input).unwrap()).unwrap();
    Command::new("node")
        .arg(script_dir.join(name))
        .arg(&input_path)
        .current_dir(repo)
        .output()
        .expect("run qa template with node")
}

fn assert_done(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "template failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // The contract: the last stdout line is {"status":"done"}.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let last = stdout.lines().last().unwrap_or("");
    let done: Value = serde_json::from_str(last)
        .unwrap_or_else(|error| panic!("last stdout line {last:?} is not JSON: {error}"));
    assert_eq!(done["status"], "done");
    done
}

fn read_report(repo: &Path, name: &str) -> Value {
    let path = repo.join(".pulse/evidence/TK-1").join(name);
    let text = fs::read_to_string(path).expect("evidence report written");
    serde_json::from_str(&text).expect("evidence report parses")
}

/// Dogfood 0025, F12: `PATCH /tasks/<id>` used to go on the wire verbatim
/// (422 uuid_parsing) and grade the product `fail` for an unfinished
/// oracle. The step must be refused as an inconclusive case naming the
/// placeholder — and the refusal happens before any app start (no run.md
/// exists here, so reaching it at all proves the short-circuit ran).
#[test]
fn api_template_refuses_a_placeholder_step_without_starting_the_app() {
    if node_missing() {
        eprintln!("skipping: node is not installed");
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let input = serde_json::json!({
        "evidence_dir": ".pulse/evidence/TK-1",
        "qa_cases": [
            {"id": "QA-001", "surface": "api",
             "steps": ["POST /tasks {\"title\":\"x\"}", "PATCH /tasks/<id> {\"status\":\"done\"}"]}
        ]
    });
    let done = assert_done(&run_template(repo.path(), "api.mjs", API_TEMPLATE, &input));
    assert_eq!(done["status"], "done");
    let report = read_report(repo.path(), "qa-api.json");
    assert_eq!(report["verdict"], "inconclusive");
    let case = &report["cases"][0];
    assert_eq!(case["id"], "QA-001");
    assert_eq!(case["status"], "inconclusive");
    let observation = case["observation"].as_str().unwrap();
    assert!(observation.contains("placeholder"), "{observation}");
    assert!(observation.contains("PATCH /tasks/<id>"), "{observation}");
    assert!(
        report["findings"].as_array().unwrap().is_empty(),
        "a refused oracle step is not a finding against the product"
    );
}

/// The placeholder refusal only applies to cases this script owns: a
/// `surface: "ui"` case with placeholder steps must pass through untouched
/// (api.mjs filters it out and proceeds to its normal config read — which,
/// with no run.md here, is the inconclusive report naming run.md, not the
/// placeholder).
#[test]
fn api_template_placeholder_check_respects_the_surface_filter() {
    if node_missing() {
        eprintln!("skipping: node is not installed");
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let input = serde_json::json!({
        "evidence_dir": ".pulse/evidence/TK-1",
        "qa_cases": [
            {"id": "QA-UI", "surface": "ui", "steps": ["open http://127.0.0.1:3000/"]}
        ]
    });
    assert_done(&run_template(repo.path(), "api.mjs", API_TEMPLATE, &input));
    let report = read_report(repo.path(), "qa-api.json");
    let observation = report["findings"][0]["summary"].as_str().unwrap();
    assert!(
        observation.contains("run.md"),
        "a ui-only input must reach the run-config read, not the placeholder refusal: {observation}"
    );
}

/// Dogfood 0025, F13: `steps[0] = "open http://127.0.0.1:3000/"` went to
/// page.goto verbatim, threw "Cannot navigate to invalid URL", and killed
/// the lane with no evidence file after minutes of app startup. The
/// template must refuse the case as inconclusive naming the step — before
/// the playwright import (which is not installed here, so reaching the
/// refusal proves it fires first) and before any app start (no run.md).
#[test]
fn ui_template_refuses_a_prose_steps0_without_starting_anything() {
    if node_missing() {
        eprintln!("skipping: node is not installed");
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let input = serde_json::json!({
        "evidence_dir": ".pulse/evidence/TK-1",
        "qa_cases": [
            {"id": "QA-006", "surface": "ui", "steps": ["open http://127.0.0.1:3000/"]}
        ]
    });
    assert_done(&run_template(repo.path(), "ui.mjs", UI_TEMPLATE, &input));
    let report = read_report(repo.path(), "qa-ui.json");
    assert_eq!(report["verdict"], "inconclusive");
    let case = &report["cases"][0];
    assert_eq!(case["id"], "QA-006");
    assert_eq!(case["status"], "inconclusive");
    let observation = case["observation"].as_str().unwrap();
    assert!(observation.contains("BARE URL"), "{observation}");
    assert!(
        observation.contains("open http://127.0.0.1:3000/"),
        "{observation}"
    );
}

/// A bare http(s) URL in steps[0] passes the check and the lane proceeds
/// past it — through the run-config read (a minimal run.md satisfies it)
/// into the legitimately failing playwright requirement, which proves the
/// URL check did not refuse a valid input. The import fails before any app
/// start, so nothing is ever launched here.
#[test]
fn ui_template_lets_a_bare_url_through_to_the_lane() {
    if node_missing() {
        eprintln!("skipping: node is not installed");
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join("docs/operations")).unwrap();
    fs::write(
        repo.path().join("docs/operations/run.md"),
        "```pulse-run\nid: ui\nstart: [\"true\"]\nready_url: \"http://127.0.0.1:3000\"\nstop: [\"true\"]\nlog: \".pulse/runtime/logs/ui.log\"\n```\n",
    )
    .unwrap();
    let input = serde_json::json!({
        "evidence_dir": ".pulse/evidence/TK-1",
        "qa_cases": [
            {"id": "QA-006", "surface": "ui", "steps": ["http://127.0.0.1:3000/"]}
        ]
    });
    let output = run_template(repo.path(), "ui.mjs", UI_TEMPLATE, &input);
    assert!(
        !output.status.success(),
        "no playwright here: the lane must stop at the import"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("qa_ui_playwright_missing"),
        "failure must be the playwright import, not the URL check: {stderr}"
    );
    assert!(
        !repo.path().join(".pulse/evidence/TK-1/qa-ui.json").exists(),
        "the import failure predates any report; the pre-existing stderr contract stands"
    );
}
