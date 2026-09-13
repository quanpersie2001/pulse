use std::fs;

use serde_json::{json, Value};

use crate::common_fixture_repo::TestRepo;

fn write_record(repo: &TestRepo, name: &str, record: &Value) -> String {
    let path = repo.path().join(name);
    fs::write(&path, serde_json::to_vec_pretty(record).unwrap()).unwrap();
    path.to_string_lossy().into_owned()
}

/// Register a document at whatever revision the registry is currently on.
///
/// The revision is read rather than assumed: `pulse init` registers
/// `DOC-GLOSSARY`, so a fresh repository does not start at revision 1, and a
/// hard-coded number breaks again the next time init seeds anything.
fn register(repo: &TestRepo, record: &Value) {
    let file = write_record(repo, "document-record.json", record);
    let revision = registry_revision(repo);
    repo.pulse_ok(&[
        "docs",
        "register",
        "--file",
        &file,
        "--expected-registry-revision",
        &revision.to_string(),
        "--actor",
        "human:test",
        "--json",
    ]);
    fs::remove_file(file).unwrap();
}

/// Current docs-registry revision.
fn registry_revision(repo: &TestRepo) -> u64 {
    repo.pulse_ok(&["docs", "status", "--json"])["registry"]["revision"]
        .as_u64()
        .expect("registry revision")
}

fn authored_record() -> Value {
    json!({
        "id": "DOC-AUTH-CONTRACT",
        "revision": 1,
        "path": "docs/product/authentication.md",
        "kind": "product",
        "status": "approved",
        "owner": "team:product",
        "summary": "Refresh-token failure contract",
        "scope": {"paths": ["src/token.mjs"]},
        "tags": [],
        "generated": null,
        "superseded_by": null
    })
}

fn generated_record(freshness_check: String) -> Value {
    json!({
        "id": "DOC-ARCH-GENERATED",
        "revision": 1,
        "path": "docs/architecture/overview.md",
        "kind": "generated",
        "status": "approved",
        "owner": "system:architecture-generator",
        "summary": "Generated architecture overview",
        "scope": {"paths": ["src/**"]},
        "tags": [],
        "generated": {
            "command": "node scripts/generate-architecture.mjs",
            "freshness_check": freshness_check
        },
        "superseded_by": null
    })
}

fn nested_pulse_show(document_id: &str) -> String {
    let binary = crate::common_bin::bin().replace('"', "\\\"");
    format!("\"{binary}\" --repo-root . docs show {document_id} --json")
}

fn failed_report(repo: &TestRepo) -> Value {
    let output = repo.pulse(&["docs", "validate", "--json"]);
    assert!(
        !output.status.success(),
        "validation unexpectedly passed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let error: Value = serde_json::from_slice(&output.stderr).expect("validation error on stderr");
    assert_eq!(error["code"], "docs_validation_failed");
    serde_json::from_slice(&output.stdout).expect("validation report on stdout")
}

fn set_generated_check(registry: &mut Value, command: String) {
    let document = registry["documents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|document| document["id"] == "DOC-ARCH-GENERATED")
        .unwrap();
    document["generated"]["freshness_check"] = json!(command);
}

#[test]
fn docs_validate_checks_declared_freshness_links_and_navigation_on_fixture_copy() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--json"]);
    register(&repo, &authored_record());
    register(
        &repo,
        &generated_record(nested_pulse_show("DOC-ARCH-GENERATED")),
    );
    repo.pulse_ok(&["docs", "index", "--json"]);

    let valid = repo.pulse_ok(&["docs", "validate", "--json"]);
    assert_eq!(valid["valid"], true);
    assert_eq!(
        valid["checks"],
        json!([
            // Two registered here plus DOC-GLOSSARY, which init seeds.
            {"kind": "registry", "result": "passed", "checked": 3},
            {"kind": "internal_links", "result": "passed", "checked": 0},
            {"kind": "generated_freshness", "result": "passed", "checked": 1},
            {"kind": "navigation_projections", "result": "passed", "checked": 1}
        ])
    );

    crate::common_git::commit_all(repo.path());
    let recorded = repo.pulse_ok(&[
        "docs",
        "validate",
        "--record",
        "--actor",
        "agent:docs-reviewer",
        "--json",
    ]);
    assert_eq!(recorded["code"], "documentation_validation_recorded");
    assert_eq!(recorded["receipt"]["receipt"]["result"], "passed");
    assert_eq!(
        recorded["receipt"]["receipt"]["payload"]["payload_version"],
        1
    );
    assert_eq!(
        recorded["receipt"]["receipt"]["payload"]["documents"][0]["document_id"],
        "DOC-ARCH-GENERATED"
    );
    assert_eq!(recorded["verification"]["registry"]["status"], "current");
    assert_eq!(
        recorded["verification"]["policy"]["status"],
        "structurally_satisfied"
    );
    assert_eq!(recorded["verification"]["gate_eligible"], true);

    let receipt_id = recorded["receipt"]["receipt"]["id"].as_str().unwrap();
    let registry_path = repo.path().join(".pulse/docs/registry.json");
    let mut registry: Value = serde_json::from_slice(&fs::read(&registry_path).unwrap()).unwrap();
    registry["documents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|document| document["id"] == "DOC-ARCH-GENERATED")
        .unwrap()["revision"] = json!(2);
    fs::write(
        &registry_path,
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    let drifted = repo.pulse(&[
        "evidence",
        "receipt",
        "verify",
        receipt_id,
        "--current",
        "--json",
    ]);
    assert!(!drifted.status.success());
    let drifted_report: Value = serde_json::from_slice(&drifted.stdout).unwrap();
    assert_eq!(drifted_report["registry"]["status"], "mismatch");
    assert!(drifted_report["registry"]["reason_codes"]
        .as_array()
        .unwrap()
        .contains(&json!("document_receipt_revision_stale")));
    registry["documents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|document| document["id"] == "DOC-ARCH-GENERATED")
        .unwrap()["revision"] = json!(1);
    fs::write(
        &registry_path,
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();

    let authored_path = repo.path().join("docs/product/authentication.md");
    let authored = fs::read_to_string(&authored_path).unwrap();
    fs::write(
        &authored_path,
        format!("{authored}\n[Missing contract](../missing.md)\n"),
    )
    .unwrap();
    let broken_link = failed_report(&repo);
    assert!(broken_link["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["code"] == "docs_internal_link_broken"));
    fs::write(&authored_path, authored).unwrap();

    let mut registry: Value = serde_json::from_slice(&fs::read(&registry_path).unwrap()).unwrap();
    set_generated_check(&mut registry, nested_pulse_show("DOC-NOT-FOUND"));
    fs::write(
        &registry_path,
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    repo.pulse_ok(&["docs", "index", "--json"]);
    let stale_generated = failed_report(&repo);
    assert!(stale_generated["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["code"] == "docs_generated_stale"));

    set_generated_check(&mut registry, nested_pulse_show("DOC-ARCH-GENERATED"));
    fs::write(
        &registry_path,
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    repo.pulse_ok(&["docs", "index", "--json"]);
    fs::remove_file(repo.path().join("docs/_index.md")).unwrap();
    let missing_projection = failed_report(&repo);
    assert!(missing_projection["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["code"] == "docs_index_projection_missing"));

    fs::write(repo.path().join("docs/_index.md"), b"# User-owned index\n").unwrap();
    let conflicting_projection = failed_report(&repo);
    assert!(conflicting_projection["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["code"] == "docs_index_projection_conflict"));
}
