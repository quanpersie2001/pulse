use serde_json::Value;
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn bin() -> String {
    std::env::var("CARGO_BIN_EXE_pulse").unwrap_or_else(|_| "target/debug/pulse".to_string())
}

fn run(repo: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args(args)
        .output()
        .expect("run pulse")
}

fn run_ok(repo: &TempDir, args: &[&str]) -> Value {
    let output = run(repo, args);
    assert!(
        output.status.success(),
        "command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

fn event_count(repo: &TempDir) -> usize {
    let events = repo.path().join(".pulse/events");
    if !events.exists() {
        return 0;
    }
    fs::read_dir(events)
        .unwrap()
        .flat_map(|date| fs::read_dir(date.unwrap().path()).unwrap())
        .filter(|entry| entry.as_ref().unwrap().path().extension().and_then(|s| s.to_str()) == Some("json"))
        .count()
}

fn kill_at_failpoint(repo: &TempDir, failpoint: &str, id: &str) {
    let mut child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg(failpoint)
        .args([
            "work",
            "edit",
            id,
            "--expected-revision",
            "1",
            "--title",
            "Crashed",
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn failpoint command");

    let start = Instant::now();
    while repo.path().join(".pulse/runtime/transactions").read_dir().unwrap().next().is_none() {
        assert!(start.elapsed() < Duration::from_secs(5), "transaction intent was not persisted");
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(100));
    child.kill().expect("kill child");
    let _ = child.wait();
}

#[test]
fn killed_after_intent_recovers_by_cleaning_uncommitted_transaction() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &["work", "create", "--kind", "ticket", "--title", "Before", "--json"],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();
    let before_events = event_count(&repo);

    kill_at_failpoint(&repo, "after_intent", &id);
    run_ok(&repo, &["graph", "recover", "--json"]);

    let shown = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(shown["node"]["revision"], 1);
    assert_eq!(shown["node"]["title"], "Before");
    assert_eq!(event_count(&repo), before_events);
}

#[test]
fn killed_after_canonical_recovers_missing_event_without_reapplying_node() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &["work", "create", "--kind", "ticket", "--title", "Before", "--json"],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();
    let before_events = event_count(&repo);

    kill_at_failpoint(&repo, "after_canonical", &id);
    let crashed = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(crashed["node"]["revision"], 2);
    assert_eq!(crashed["node"]["title"], "Crashed");

    run_ok(&repo, &["graph", "recover", "--json"]);
    let recovered = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(recovered["node"]["revision"], 2);
    assert_eq!(recovered["node"]["title"], "Crashed");
    assert_eq!(event_count(&repo), before_events + 1);

    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(event_count(&repo), before_events + 1);
}

#[test]
fn killed_after_event_recovers_by_cleaning_intent_without_duplicate_event() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &["work", "create", "--kind", "ticket", "--title", "Before", "--json"],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();
    let before_events = event_count(&repo);

    kill_at_failpoint(&repo, "after_event", &id);
    assert_eq!(event_count(&repo), before_events + 1);
    run_ok(&repo, &["graph", "recover", "--json"]);
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(event_count(&repo), before_events + 1);
}
