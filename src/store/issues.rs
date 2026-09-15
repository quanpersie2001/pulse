//! JSONL store for `.pulse/issues.jsonl` (plan 0022 §4).
//!
//! One canonical JSON object per line, sorted by `id` on write. Every
//! mutation reads the whole file, transforms it in memory, validates every
//! resulting record against the embedded schema, then writes through
//! `storage::atomic_write` under the repository write lock — plan §4.1's
//! "một mutation = một event" is the kernel's job once it wires event
//! emission on top of [`mutate`]; this module only owns the file.
//!
//! A line that fails to parse fails the whole read with
//! `issues_line_invalid` naming the line number. Track B / Decision 0017
//! learned the alternative (silently drop the bad line) hides real damage
//! from every caller downstream.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use jsonschema::{Draft, JSONSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{PulseError, Result};
use crate::storage::{self, WriteGuard};

const SCHEMA_JSON: &str = include_str!("../schema/issue.schema.json");

/// Path to the single store file, relative to the repository root.
pub fn issues_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".pulse/issues.jsonl")
}

/// Fields every record carries regardless of `kind` (plan §4.3). Specific
/// kinds have their own typed views built on demand from the underlying
/// [`Value`] (see `kernel::ready`, `kernel::completion`); the store itself
/// only needs enough structure to sort, look up and CAS-check records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonFields {
    pub schema: u32,
    pub id: String,
    pub kind: String,
    pub status: String,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// Extract the common fields every record must have.
///
/// # Errors
/// Returns `issues_record_invalid` when a required common field is missing
/// or the wrong type — this runs after schema validation in [`mutate`], so
/// in practice it only fires for records a caller built directly in tests.
pub fn common_fields(record: &Value) -> Result<CommonFields> {
    serde_json::from_value(record.clone()).map_err(|error| {
        PulseError::kernel(
            "issues_record_invalid",
            format!("record is missing a common field: {error}"),
            "every record needs schema, id, kind, status, revision, created_at, updated_at",
        )
    })
}

/// Read every record in `issues.jsonl`, in file order (not sorted; callers
/// that need id order get it from [`mutate`], which always writes sorted).
///
/// Blank lines and lines starting with `#` are skipped on read but are never
/// written back (plan §4.1) — [`mutate`] rewrites the whole file from
/// records it validated, so a comment line present today disappears the
/// next time anything mutates the store. A missing file reads as empty.
///
/// # Errors
/// `issues_line_invalid` names the 1-based line number of the first line
/// that is not valid JSON. Never silently skipped.
pub fn read_all(repo_root: &Path) -> Result<Vec<Value>> {
    let path = issues_path(repo_root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
    let text = String::from_utf8(bytes).map_err(|error| {
        PulseError::kernel(
            "issues_line_invalid",
            format!("{} is not valid UTF-8: {error}", path.display()),
            "issues.jsonl must be UTF-8 text; restore it from git history",
        )
    })?;

    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let value: Value = serde_json::from_str(line).map_err(|error| {
            PulseError::kernel(
                "issues_line_invalid",
                format!(
                    "{}:{} is not valid JSON: {error}",
                    path.display(),
                    index + 1
                ),
                "fix or remove the offending line by hand; issues.jsonl never skips a bad line silently",
            )
        })?;
        records.push(value);
    }
    Ok(records)
}

fn validator() -> &'static JSONSchema {
    static VALIDATOR: OnceLock<JSONSchema> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema: Value =
            serde_json::from_str(SCHEMA_JSON).expect("embedded issue schema is valid JSON");
        JSONSchema::options()
            .with_draft(Draft::Draft202012)
            .compile(&schema)
            .expect("embedded issue schema compiles")
    })
}

/// Validate one record against `src/schema/issue.schema.json`.
///
/// # Errors
/// `issues_schema_invalid` lists every violation (not just the first), each
/// naming the JSON pointer path that failed.
pub fn validate_record(record: &Value) -> Result<()> {
    let outcome = validator().validate(record);
    let Err(errors) = outcome else {
        return Ok(());
    };
    let messages: Vec<String> = errors
        .take(16)
        .map(|error| format!("{}: {error}", error.instance_path))
        .collect();
    Err(PulseError::kernel(
        "issues_schema_invalid",
        format!("record fails schema: {}", messages.join("; ")),
        "run `pulse show <id>` after fixing the field named by the error path",
    ))
}

/// Read-validate-write one mutation under the repository write lock.
///
/// `transform` receives every current record (file order) and returns the
/// next full set of records. Every record in the result is schema-validated
/// before anything is written; a validation failure leaves `issues.jsonl`
/// byte-for-byte untouched. The result is sorted by `id` before it is
/// written, matching plan §4.1.
///
/// # Errors
/// Propagates `transform`'s error, any `issues_schema_invalid` from the
/// records it returns, or an I/O error acquiring the lock or writing the
/// file.
pub fn mutate<F>(repo_root: &Path, transform: F) -> Result<Vec<Value>>
where
    F: FnOnce(Vec<Value>) -> Result<Vec<Value>>,
{
    let _guard = WriteGuard::acquire(repo_root)?;
    let current = read_all(repo_root)?;
    let mut next = transform(current)?;
    for record in &next {
        validate_record(record)?;
    }
    next.sort_by(|a, b| record_id(a).cmp(record_id(b)));
    write_all(repo_root, &next)?;
    Ok(next)
}

fn record_id(record: &Value) -> &str {
    record.get("id").and_then(Value::as_str).unwrap_or("")
}

fn write_all(repo_root: &Path, records: &[Value]) -> Result<()> {
    let path = issues_path(repo_root);
    let mut bytes = Vec::new();
    for record in records {
        bytes.extend_from_slice(&crate::canonical_json::to_canonical_line_bytes(record)?);
        bytes.push(b'\n');
    }
    storage::atomic_write(&path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_repo() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn ticket(id: &str, revision: u64) -> Value {
        json!({
            "schema": 3,
            "id": id,
            "kind": "ticket",
            "title": "Do the thing",
            "status": "draft",
            "revision": revision,
            "created_at": "2026-09-16T00:00:00Z",
            "updated_at": "2026-09-16T00:00:00Z",
            "role": "implementation",
        })
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let repo = temp_repo();
        let records = read_all(repo.path()).expect("read");
        assert!(records.is_empty());
    }

    #[test]
    fn write_then_read_round_trips_and_sorts_by_id() {
        let repo = temp_repo();
        mutate(repo.path(), |records| {
            assert!(records.is_empty());
            Ok(vec![ticket("TK-bbbb", 1), ticket("TK-aaaa", 1)])
        })
        .expect("mutate");

        let read_back = read_all(repo.path()).expect("read");
        let ids: Vec<&str> = read_back.iter().map(record_id).collect();
        assert_eq!(ids, ["TK-aaaa", "TK-bbbb"]);
    }

    #[test]
    fn blank_and_comment_lines_are_skipped_on_read() {
        let repo = temp_repo();
        let path = issues_path(repo.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!(
                "# a comment\n\n{}\n",
                serde_json::to_string(&ticket("TK-aaaa", 1)).unwrap()
            ),
        )
        .unwrap();

        let records = read_all(repo.path()).expect("read");
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn a_line_that_is_not_json_fails_the_whole_read_naming_the_line() {
        let repo = temp_repo();
        let path = issues_path(repo.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!(
                "{}\nnot json at all\n",
                serde_json::to_string(&ticket("TK-aaaa", 1)).unwrap()
            ),
        )
        .unwrap();

        let err = read_all(repo.path()).unwrap_err();
        assert_eq!(err.code(), "issues_line_invalid");
        assert!(err.to_string().contains(":2"));
        assert!(err.hint().is_some());
    }

    #[test]
    fn invalid_record_is_rejected_and_the_file_stays_untouched() {
        let repo = temp_repo();
        mutate(repo.path(), |_| Ok(vec![ticket("TK-aaaa", 1)])).expect("seed");

        let mut broken = ticket("TK-bbbb", 1);
        broken["status"] = json!("");
        let err = mutate(repo.path(), |mut records| {
            records.push(broken.clone());
            Ok(records)
        })
        .unwrap_err();
        assert_eq!(err.code(), "issues_schema_invalid");
        assert!(err.hint().is_some());

        let after = read_all(repo.path()).expect("read");
        assert_eq!(after.len(), 1, "failed mutation must not touch the file");
    }

    #[test]
    fn ticket_without_role_is_rejected_by_schema() {
        let mut broken = ticket("TK-aaaa", 1);
        broken.as_object_mut().unwrap().remove("role");
        let err = validate_record(&broken).unwrap_err();
        assert_eq!(err.code(), "issues_schema_invalid");
    }

    #[test]
    fn decision_requires_a_decision_field_and_context_is_a_string() {
        let decision = json!({
            "schema": 3,
            "id": "DEC-aaaa",
            "kind": "decision",
            "title": "Pick an approach",
            "status": "proposed",
            "revision": 1,
            "created_at": "2026-09-16T00:00:00Z",
            "updated_at": "2026-09-16T00:00:00Z",
            "context": "why we need to decide",
            "decision": "we picked option A",
        });
        validate_record(&decision).expect("valid decision record");

        let mut missing_decision = decision.clone();
        missing_decision.as_object_mut().unwrap().remove("decision");
        assert_eq!(
            validate_record(&missing_decision).unwrap_err().code(),
            "issues_schema_invalid"
        );
    }

    #[test]
    fn mutate_serializes_concurrent_callers_through_the_write_lock() {
        let repo = temp_repo();
        mutate(repo.path(), |_| Ok(vec![ticket("TK-aaaa", 1)])).expect("seed");

        let lock_path = repo.path().join(".pulse/runtime/locks/workgraph.lock");
        assert!(
            lock_path.exists(),
            "mutate must acquire the shared repository write lock"
        );
    }

    #[test]
    fn concurrent_mutations_from_two_threads_never_lose_a_write() {
        let repo = temp_repo();
        mutate(repo.path(), |_| Ok(Vec::new())).expect("seed empty store");

        let repo_path = repo.path().to_path_buf();
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let repo_path = repo_path.clone();
                std::thread::spawn(move || {
                    let id = format!("TK-{i:04x}");
                    mutate(&repo_path, move |mut records| {
                        records.push(ticket(&id, 1));
                        Ok(records)
                    })
                    .expect("mutate")
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("thread");
        }

        let records = read_all(&repo_path).expect("read");
        assert_eq!(
            records.len(),
            8,
            "every concurrent writer's record must survive"
        );
    }
}
