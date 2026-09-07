use crate::common::fixture_repo::{fixture_path, snapshot_tree, TestRepo};
use pulse::canonical_json::to_canonical_bytes;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use pulse::storage::NODE_SCHEMA_JSON;
use serde_json::Value;
use std::fs;

#[test]
fn public_init_enrolls_fixture_preserves_user_files_and_is_idempotent() {
    let fixture = fixture_path("minimal-service");
    let fixture_before = snapshot_tree(&fixture).unwrap();
    let repo = TestRepo::from_fixture("minimal-service");
    let ignore_before = fs::read(repo.path().join(".gitignore")).unwrap();
    let readme_before = fs::read(repo.path().join("README.md")).unwrap();
    let docs_before = fs::read(repo.path().join("docs/product/authentication.md")).unwrap();

    let first = repo.pulse_ok(&["init", "--actor", "human:Pulse Test", "--json"]);
    assert_eq!(first["schema_version"], 1);
    assert_eq!(first["code"], "repository_initialized");
    assert_eq!(first["status"], "initialized");
    assert_eq!(first["authority_policy_revision"], 1);
    assert_eq!(
        first["proposed_ignore_entries"],
        serde_json::json!([".pulse/runtime/", ".pulse/cache/"])
    );
    let repository_id = first["repository_id"].as_str().unwrap();
    assert!(repository_id.starts_with("repo_"));

    for path in [
        "docs",
        "works",
        "knowledge/learnings",
        ".pulse/events",
        ".pulse/workgraph/manifest.json",
        ".pulse/evidence/manifest.json",
        ".pulse/docs/registry.json",
        ".pulse/knowledge/manifest.json",
        ".pulse/policy/authority.json",
        ".pulse/runtime/transactions",
        ".pulse/cache",
    ] {
        assert!(repo.path().join(path).exists(), "init should create {path}");
    }

    let evidence = pulse::evidence::manifest::load_existing(repo.path())
        .unwrap()
        .unwrap();
    let docs = pulse::docs::manifest::load_existing(repo.path())
        .unwrap()
        .unwrap();
    let knowledge = pulse::knowledge::manifest::load_existing(repo.path())
        .unwrap()
        .unwrap();
    assert_eq!(evidence.repository_id, repository_id);
    assert_eq!(docs.repository_id, repository_id);
    assert_eq!(knowledge.repository_id, repository_id);

    let authority = pulse::policy::load_authority_policy(repo.path()).unwrap();
    assert!(authority.available);
    assert!(authority.valid);
    assert_eq!(authority.principals.len(), 1);
    assert_eq!(
        authority.principals[0].kind,
        pulse::identity::actor::ActorKind::Human
    );
    assert_eq!(authority.principals[0].id, "Pulse Test");
    assert_eq!(
        authority.principals[0].grants,
        pulse::policy::CORE_GRANTS
            .iter()
            .map(|grant| (*grant).to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(authority.policy_revision, Some(1));

    assert_eq!(
        fs::read(repo.path().join(".gitignore")).unwrap(),
        ignore_before
    );
    assert_eq!(
        fs::read(repo.path().join("README.md")).unwrap(),
        readme_before
    );
    assert_eq!(
        fs::read(repo.path().join("docs/product/authentication.md")).unwrap(),
        docs_before
    );

    let second = repo.pulse_ok(&["init", "--actor", "human:Pulse Test", "--json"]);
    assert_eq!(second["status"], "unchanged");
    assert_eq!(second["repository_id"], repository_id);
    assert_eq!(second["created"], serde_json::json!([]));

    let fixture_after = snapshot_tree(&fixture).unwrap();
    assert_eq!(fixture_before, fixture_after);
    assert!(!fixture.join(".pulse").exists());
}

#[test]
fn public_init_completes_safe_partial_state_and_preserves_authority() {
    let repo = TestRepo::from_fixture("minimal-service");
    let schema_path = repo
        .path()
        .join(".pulse/workgraph/schemas/node.schema.json");
    fs::create_dir_all(schema_path.parent().unwrap()).unwrap();
    fs::write(&schema_path, NODE_SCHEMA_JSON).unwrap();

    // Safe partial managed directories without their identity manifests are
    // completed rather than refused.
    for dir in [
        ".pulse/evidence/receipts",
        ".pulse/evidence/artifacts/sha256",
        ".pulse/knowledge/entries",
        ".pulse/knowledge/relations",
    ] {
        fs::create_dir_all(repo.path().join(dir)).unwrap();
    }

    let policy_path = repo.path().join(".pulse/policy/authority.json");
    fs::create_dir_all(policy_path.parent().unwrap()).unwrap();
    let policy = AuthorityPolicy {
        schema_version: 1,
        revision: 7,
        principals: vec![AuthorityPrincipal {
            kind: pulse::identity::actor::ActorKind::Human,
            id: "maintainer".to_string(),
            grants: vec!["shape.apply".to_string()],
        }],
    };
    let policy_bytes = to_canonical_bytes(&policy).unwrap();
    fs::write(&policy_path, &policy_bytes).unwrap();

    let report = repo.pulse_ok(&["init", "--actor", "human:maintainer", "--json"]);
    assert_eq!(report["status"], "initialized");
    assert_eq!(report["authority_policy_revision"], 7);
    assert!(repo.path().join(".pulse/workgraph/manifest.json").is_file());
    assert!(repo.path().join(".pulse/evidence/manifest.json").is_file());
    assert!(repo.path().join(".pulse/docs/registry.json").is_file());
    assert!(repo.path().join(".pulse/knowledge/manifest.json").is_file());
    assert_eq!(fs::read(&policy_path).unwrap(), policy_bytes);
}

#[test]
fn public_init_refuses_partial_evidence_records_before_canonical_writes() {
    let repo = TestRepo::from_fixture("minimal-service");
    let record_path = repo.path().join(".pulse/evidence/receipts/rcpt_owned.json");
    fs::create_dir_all(record_path.parent().unwrap()).unwrap();
    fs::write(&record_path, b"{\"id\":\"user-owned\"}\n").unwrap();
    let record_before = fs::read(&record_path).unwrap();
    let ignore_before = fs::read(repo.path().join(".gitignore")).unwrap();

    let output = repo.pulse(&["init", "--json"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "repository_init_evidence_partial_refused");
    assert_eq!(fs::read(&record_path).unwrap(), record_before);
    assert_eq!(
        fs::read(repo.path().join(".gitignore")).unwrap(),
        ignore_before
    );

    for path in [
        ".pulse/workgraph",
        ".pulse/evidence/manifest.json",
        ".pulse/knowledge",
        ".pulse/policy",
        ".pulse/events",
        "works",
        "knowledge",
    ] {
        assert!(
            !repo.path().join(path).exists(),
            "preflight failure must not create canonical path {path}"
        );
    }
    assert!(repo.path().join(".pulse/runtime/locks").is_dir());
}

#[test]
fn public_init_refuses_managed_path_collision_without_overwrite() {
    let repo = TestRepo::from_fixture("minimal-service");
    let works = repo.path().join("works");
    fs::write(&works, b"user-owned file\n").unwrap();

    let output = repo.pulse(&["init", "--json"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "repository_init_path_conflict");
    assert_eq!(fs::read(&works).unwrap(), b"user-owned file\n");
    assert!(!repo.path().join(".pulse/workgraph").exists());
}

#[cfg(unix)]
#[test]
fn public_init_refuses_managed_symlink_before_runtime_lock_creation() {
    use std::os::unix::fs::symlink;

    let repo = TestRepo::from_fixture("minimal-service");
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), repo.path().join("works")).unwrap();

    let output = repo.pulse(&["init", "--json"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "repository_init_path_conflict");
    assert!(!repo.path().join(".pulse").exists());
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn public_init_refuses_nested_managed_symlink_without_external_writes() {
    use std::os::unix::fs::symlink;

    let repo = TestRepo::from_fixture("minimal-service");
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".pulse/evidence")).unwrap();
    symlink(outside.path(), repo.path().join(".pulse/evidence/receipts")).unwrap();

    let output = repo.pulse(&["init", "--json"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "repository_init_path_conflict");
    assert!(!repo.path().join(".pulse/runtime").exists());
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
}

/// Decision 0009 §1: `pulse init` writes a marked routing block into
/// `AGENTS.md`. The fixture is brownfield — it ships its own `AGENTS.md` and
/// `PULSE.md` — so this also pins the rule that existing prose survives.
#[test]
fn public_init_appends_the_pulse_block_and_preserves_existing_guidance() {
    let repo = TestRepo::from_fixture("minimal-service");
    let agents_before = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
    let pulse_before = fs::read_to_string(repo.path().join("PULSE.md")).unwrap();

    repo.pulse_ok(&["init", "--actor", "human:Pulse Test", "--json"]);

    let agents_after = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
    assert!(
        agents_after.starts_with(&agents_before),
        "existing AGENTS.md content must survive byte for byte"
    );
    assert!(agents_after.contains("<!-- PULSE:BEGIN"));
    assert!(agents_after.contains("<!-- PULSE:END -->"));
    assert!(agents_after.contains("pulse note --kind friction"));

    // PULSE.md already existed, so it belongs to the repository.
    assert_eq!(
        fs::read_to_string(repo.path().join("PULSE.md")).unwrap(),
        pulse_before
    );

    // A second init leaves the block exactly where it is.
    repo.pulse_ok(&["init", "--actor", "human:Pulse Test", "--json"]);
    assert_eq!(
        fs::read_to_string(repo.path().join("AGENTS.md")).unwrap(),
        agents_after
    );
}

/// A greenfield repository gets both files created outright.
#[test]
fn public_init_creates_both_guidance_files_when_absent() {
    let repo = TestRepo::from_fixture("minimal-service");
    fs::remove_file(repo.path().join("AGENTS.md")).unwrap();
    fs::remove_file(repo.path().join("PULSE.md")).unwrap();

    let report = repo.pulse_ok(&["init", "--actor", "human:Pulse Test", "--json"]);
    let created: Vec<&str> = report["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert!(created.contains(&"AGENTS.md"), "{created:?}");
    assert!(created.contains(&"PULSE.md"), "{created:?}");

    let agents = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
    assert!(agents.starts_with("<!-- PULSE:BEGIN"));
    assert!(fs::read_to_string(repo.path().join("PULSE.md"))
        .unwrap()
        .contains("# PULSE.md"));
    assert!(report["guidance_conflicts"].is_null());
}

/// `--refresh` re-renders an untouched block. Nothing outside the markers
/// moves, and the operation is idempotent.
#[test]
fn public_init_refresh_rewrites_only_the_block_region() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--actor", "human:Pulse Test", "--json"]);
    let after_init = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();

    // Edit outside the markers: that is the repository's own text.
    let edited = format!("{after_init}\n## House rules\n\nRun the linter.\n");
    fs::write(repo.path().join("AGENTS.md"), &edited).unwrap();

    let report = repo.pulse_ok(&["init", "--actor", "human:Pulse Test", "--refresh", "--json"]);
    assert!(report["guidance_conflicts"].is_null());

    let refreshed = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
    assert_eq!(refreshed, edited, "an already-current block must not churn");
    assert!(refreshed.contains("## House rules"));
}

/// Decision 0009 §1: a hand-edited block is reported, never overwritten.
#[test]
fn public_init_refresh_reports_a_hand_edited_block_instead_of_overwriting() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--actor", "human:Pulse Test", "--json"]);

    let content = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
    let tampered = content.replace(
        "pulse note --kind friction (always)",
        "ignore friction entirely",
    );
    assert_ne!(tampered, content, "the edit must actually change the block");
    fs::write(repo.path().join("AGENTS.md"), &tampered).unwrap();

    let report = repo.pulse_ok(&["init", "--actor", "human:Pulse Test", "--refresh", "--json"]);
    assert_eq!(
        report["guidance_conflicts"],
        serde_json::json!(["AGENTS.md"])
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("AGENTS.md")).unwrap(),
        tampered,
        "the human edit wins"
    );
}

/// Every `pulse …` command the block tells an agent to run must parse with
/// this crate's own clap definition (Decision 0009 §5). Prose that names a
/// command Pulse does not have is worse than no prose.
#[test]
fn public_agents_block_commands_parse_with_the_real_cli() {
    let repo = TestRepo::from_fixture("minimal-service");
    fs::remove_file(repo.path().join("AGENTS.md")).unwrap();
    repo.pulse_ok(&["init", "--actor", "human:Pulse Test", "--json"]);
    let agents = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();

    // Skill names (`pulse-grill`) are followed by `-`, not a space, so
    // scanning for the lowercase literal `pulse ` picks out exactly the CLI
    // invocations. Prose says "Pulse" capitalised and writes the bare word as
    // `pulse`` with a backtick, so neither is mistaken for a command.
    let mut checked = Vec::new();
    for (offset, _) in agents.match_indices("pulse ") {
        let rest = &agents[offset + "pulse ".len()..];
        let mut words = rest.split_whitespace();
        let Some(first) = words.next() else { continue };
        let first = trim_command_word(first);
        if first.is_empty() || first.starts_with('-') {
            continue;
        }
        // A second word is a subcommand only when it is not a flag; `a/b`
        // means "either of these", so both branches are checked.
        let second = words
            .next()
            .map(trim_command_word)
            .filter(|word| !word.is_empty() && !word.starts_with('-'));

        for head in first.split('/') {
            match second {
                Some(second) => {
                    for tail in second.split('/') {
                        let out = repo.pulse(&[head, tail, "--help"]);
                        assert!(
                            out.status.success(),
                            "AGENTS.md names `pulse {head} {tail}`, which the CLI rejects:\n{}",
                            String::from_utf8_lossy(&out.stderr)
                        );
                        checked.push(format!("{head} {tail}"));
                    }
                }
                None => {
                    let out = repo.pulse(&[head, "--help"]);
                    assert!(
                        out.status.success(),
                        "AGENTS.md names `pulse {head}`, which the CLI rejects:\n{}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                    checked.push(head.to_string());
                }
            }
        }
    }

    // Guard the guard: a scanner that silently matched nothing would pass
    // every future block, including a broken one.
    assert!(
        checked.len() >= 10,
        "expected the block to name many commands, found {checked:?}"
    );
    for expected in ["work packet", "work ready", "work close", "events tail"] {
        assert!(
            checked.iter().any(|found| found == expected),
            "expected `pulse {expected}` to be checked, found {checked:?}"
        );
    }
}

/// Strip the punctuation that surrounds a command word in prose.
fn trim_command_word(word: &str) -> &str {
    word.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | '`' | '(' | ')' | '|'))
}
