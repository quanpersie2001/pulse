use pulse::evidence::model::{ActorKind, ActorRef};
use pulse::policy::{load_authority_policy, validate_authority_policy_file, AuthorityPolicy};
use std::fs;

fn write_policy(repo: &std::path::Path, policy: &AuthorityPolicy) {
    let path = repo.join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        pulse::canonical_json::to_canonical_bytes(policy).unwrap(),
    )
    .unwrap();
}

#[test]
fn missing_authority_policy_is_unavailable_default_deny() {
    let repo = tempfile::tempdir().unwrap();
    let report = load_authority_policy(repo.path()).unwrap();
    assert!(!report.available);
    assert!(!report.valid);
    assert_eq!(report.reason_codes, vec!["readiness_policy_missing"]);
}

#[test]
fn policy_validates_fingerprints_and_queries_exact_grants() {
    let repo = tempfile::tempdir().unwrap();
    let policy = AuthorityPolicy {
        schema_version: 1,
        revision: 1,
        principals: vec![pulse::policy::AuthorityPrincipal {
            kind: ActorKind::Human,
            id: "quannv".to_string(),
            grants: vec![
                "decision.accept".to_string(),
                "shape.approve.R0".to_string(),
            ],
        }],
    };
    write_policy(repo.path(), &policy);

    let report = validate_authority_policy_file(repo.path()).unwrap();
    assert!(report.available);
    assert!(report.valid, "{:?}", report.reason_codes);
    assert_eq!(report.policy_revision, Some(1));
    assert!(report.fingerprint.as_ref().unwrap().starts_with("sha256:"));

    let loaded = AuthorityPolicy {
        schema_version: 1,
        revision: report.policy_revision.unwrap(),
        principals: report.principals.clone(),
    };
    assert!(loaded.has_grant(
        &ActorRef {
            kind: ActorKind::Human,
            id: "quannv".to_string(),
        },
        "decision.accept"
    ));
    assert!(!loaded.has_grant(
        &ActorRef {
            kind: ActorKind::Human,
            id: "someone".to_string(),
        },
        "decision.accept"
    ));
}

#[test]
fn wildcard_or_noncanonical_policy_is_invalid() {
    let repo = tempfile::tempdir().unwrap();
    let path = repo.path().join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        br#"{
          "schema_version": 1,
          "revision": 1,
          "principals": [
            {"kind": "human", "id": "quannv", "grants": ["*"]}
          ]
        }
        "#,
    )
    .unwrap();

    let report = validate_authority_policy_file(repo.path()).unwrap();
    assert!(report.available);
    assert!(!report.valid);
    assert!(report
        .reason_codes
        .contains(&"readiness_policy_invalid".to_string()));
    assert!(report
        .reason_codes
        .contains(&"readiness_policy_not_canonical".to_string()));
}
