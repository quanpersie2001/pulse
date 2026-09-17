use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::error::PulseError;
use pulse::storage::atomic::atomic_replace;
use pulse::storage::paths::resolve_repo_relative;
use serde_json::json;
use std::fs;

#[test]
fn canonical_json_is_deterministic_and_lf_terminated() {
    let a = json!({"z": 1, "a": {"b": 2, "a": [3, 2, 1]}});
    let b = json!({"a": {"a": [3, 2, 1], "b": 2}, "z": 1});

    let a_bytes = to_canonical_bytes(&a).unwrap();
    let b_bytes = to_canonical_bytes(&b).unwrap();

    assert_eq!(a_bytes, b_bytes);
    assert_eq!(hash_bytes(&a_bytes), hash_bytes(&b_bytes));
    assert!(a_bytes.ends_with(b"\n"));
    assert!(!a_bytes.ends_with(b"\n\n"));
    assert!(!String::from_utf8(a_bytes).unwrap().contains("\r\n"));
}

#[test]
fn canonical_json_rejects_float_numbers() {
    let error = to_canonical_bytes(&json!({"decimal_number": 1.5})).unwrap_err();
    assert!(matches!(error, PulseError::FloatRejected { .. }));
}

#[test]
fn safe_paths_reject_traversal_and_symlink_escape() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();

    let traversal = resolve_repo_relative(repo, "works/../secret").unwrap_err();
    assert!(matches!(traversal, PulseError::PathTraversal { .. }));

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        let link = repo.join("link-out");
        symlink(outside.path(), &link).unwrap();
        let escape = resolve_repo_relative(repo, "link-out/file.md").unwrap_err();
        assert!(matches!(escape, PulseError::PathEscape { .. }));
    }
}

#[test]
fn atomic_replace_writes_same_directory_and_replaces_existing() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("node.json");
    fs::write(&target, b"before\n").unwrap();

    let report = atomic_replace(&target, b"after\n").unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"after\n");
    assert_eq!(report.temp_path.parent(), target.parent());
    assert!(!report.temp_path.exists());
}
