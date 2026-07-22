use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;
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

fn write_json(path: &Path, value: &Value) -> String {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    path.to_string_lossy().to_string()
}

fn setup_repo_with_learning() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    let work = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Knowledge source",
            "--json",
        ],
    );
    let work_id = work["value"]["id"].as_str().unwrap();
    let draft = json!({
        "title": "Token rotation requires atomic mutation",
        "kind": "failure_pattern",
        "severity": "high",
        "summary": "Concurrent refresh can issue invalid tokens when rotation uses check-then-act.",
        "guidance": {
            "do": ["Use an atomic state transition."],
            "avoid": ["Do not split rotation into unguarded read then write."],
            "required_checks": ["Exercise concurrent refresh attempts."]
        },
        "applicability": {
            "paths": ["src/auth/**"],
            "symbols": ["rotateRefreshToken"],
            "risks": ["concurrency"]
        },
        "provenance_targets": [{
            "relation": "derived_from",
            "kind": "work",
            "id": work_id,
            "revision": 1,
            "content_hash": null
        }],
        "source_commits": [],
        "routing": null,
        "promotion": null,
        "freshness": null,
        "trust": null,
        "content": null
    });
    let draft_file = write_json(&repo.path().join("learning.json"), &draft);
    run_ok(
        &repo,
        &[
            "knowledge",
            "create",
            "--file",
            &draft_file,
            "--actor",
            "human:test",
            "--json",
        ],
    );
    repo
}

#[test]
fn knowledge_export_rebuilds_deleted_corrupt_and_stale_projection_cache_deterministically() {
    let repo = setup_repo_with_learning();
    let cache = repo.path().join(".pulse/cache/knowledge.snapshot.json");

    let first = run_ok(&repo, &["knowledge", "export", "--json"]);
    let first_cache = fs::read(&cache).unwrap();
    assert_eq!(first["schema_version"], 1);
    assert_eq!(first["counts"]["entries"], 1);
    assert_eq!(first["counts"]["relations"], 1);
    assert_eq!(
        first["eligibility"]["future_default_search"]["eligible"],
        json!([])
    );
    assert_eq!(
        first["eligibility"]["future_default_search"]["excluded"][0]["id"],
        "LRN-001"
    );
    let fingerprint = first["knowledge_fingerprint"].as_str().unwrap().to_string();

    let current = run_ok(&repo, &["knowledge", "status", "--json"]);
    assert_eq!(current["knowledge_fingerprint"], fingerprint);
    assert_eq!(current["cache_state"], "current");

    fs::remove_file(&cache).unwrap();
    let missing = run_ok(&repo, &["knowledge", "status", "--json"]);
    assert_eq!(missing["knowledge_fingerprint"], fingerprint);
    assert_eq!(missing["cache_state"], "missing");
    let rebuilt_from_missing = run_ok(&repo, &["knowledge", "export", "--json"]);
    assert_eq!(rebuilt_from_missing, first);
    assert_eq!(fs::read(&cache).unwrap(), first_cache);

    fs::write(&cache, b"not json").unwrap();
    let corrupt = run_ok(&repo, &["knowledge", "status", "--json"]);
    assert_eq!(corrupt["knowledge_fingerprint"], fingerprint);
    assert_eq!(corrupt["cache_state"], "corrupt");
    let rebuilt_from_corrupt = run_ok(&repo, &["knowledge", "export", "--json"]);
    assert_eq!(rebuilt_from_corrupt, first);
    assert_eq!(fs::read(&cache).unwrap(), first_cache);

    let mut stale_snapshot = first.clone();
    stale_snapshot["knowledge_fingerprint"] =
        json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
    fs::write(
        &cache,
        pulse::canonical_json::to_canonical_bytes(&stale_snapshot).unwrap(),
    )
    .unwrap();
    let stale = run_ok(&repo, &["knowledge", "status", "--json"]);
    assert_eq!(stale["knowledge_fingerprint"], fingerprint);
    assert_eq!(stale["cache_state"], "stale");
    let rebuilt_from_stale = run_ok(&repo, &["knowledge", "export", "--json"]);
    assert_eq!(rebuilt_from_stale, first);
    assert_eq!(fs::read(&cache).unwrap(), first_cache);
}
