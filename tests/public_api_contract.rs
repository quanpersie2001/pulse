//! Cross-domain compile-time public-path compatibility baseline.
//!
//! Locks the Rust paths that integration coverage, benches and
//! `src/bin/pulse.rs` currently import across Pulse v3 domains. Deliberately
//! lightweight: compile and a few stable constants/constructors, not an
//! exhaustive API snapshot.
//!
//! Rewritten for plan 0022 P1.3/P1.4: the entire v2 surface this file used
//! to guard (`docs`, `graph`/`JsonGraphStore`, `knowledge`, `policy`,
//! `qa`, `execution`, `work_packet`, `evidence::model`,
//! `evidence::receipt::*`, `kernel::completion`/`story_completion`) is
//! deleted. It also absorbs the old `tests/graph/public_api_paths.rs`,
//! which guarded the same kind of baseline for the graph-specific surface
//! that no longer exists — keeping two near-identical files once both are
//! mostly deleted content would be redundant.

use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::event::{EventActor, EventActorKind, EventCorrelation, EventEnvelope, EventSubject};
use pulse::id::{
    generate_hash_id, generate_learning_hash_id, validate_hash_id_for_kind, WorkId, WorkKind,
};
use pulse::identity::actor::{parse_actor, resolve_actor, ActorKind, ActorRef};
use pulse::kernel::issues::{DepType, NoteKind};
use pulse::kernel::ready::{evaluate as evaluate_ready, ReadyReport, ReadyViolation};
use pulse::kernel::roles::{authorize, Action};
use pulse::source::{same as source_same, snapshot as source_snapshot, state_repo_root, Source};
use pulse::storage::{atomic_write, safe_repo_relative};
use pulse::store::issues::{common_fields, mutate as issues_mutate, read_all, validate_record};
use pulse::{PulseError, PulseResult, Result};

#[test]
fn identity_event_storage_and_canonical_json_public_paths_compile() {
    let repo = tempfile::tempdir().unwrap();

    let actor = EventActor::new(EventActorKind::Human, "tester");
    let subject = EventSubject::new("ticket", "TK-a3f9", Some(1));
    let event = EventEnvelope::new_typed(
        "evt_01J00000000000000000000000",
        "issue.updated",
        actor,
        subject,
        Some(EventCorrelation {
            run_id: None,
            lease_id: None,
            transaction_id: None,
            receipt_id: None,
        }),
        serde_json::json!({"ok": true}),
        chrono::Utc::now(),
    );
    assert_eq!(event.schema_version, 1);

    let neutral = ActorRef {
        kind: ActorKind::Human,
        id: "quan".to_string(),
    };
    assert_eq!(neutral.as_kind_id(), "human:quan");
    assert!(parse_actor("human:quan").is_ok());
    assert!(resolve_actor(repo.path(), Some("human:quan")).is_ok());

    let hash_id = generate_hash_id(WorkKind::Ticket, "title", "2026-09-16T00:00:00Z");
    assert!(validate_hash_id_for_kind(hash_id.as_str(), WorkKind::Ticket).is_ok());
    assert!(WorkId::new(hash_id.as_str()).is_ok());
    assert!(generate_learning_hash_id("title", "2026-09-16T00:00:00Z").starts_with("LRN-"));

    assert!(atomic_write(&repo.path().join("f.txt"), b"x").is_ok());
    assert!(safe_repo_relative("docs/x.md").is_ok());

    let bytes = to_canonical_bytes(&serde_json::json!({"a": 1})).unwrap();
    assert!(!hash_bytes(&bytes).is_empty());

    fn accepts_result(_: PulseResult<()>) {}
    fn accepts_alias(_: Result<()>) {}
    accepts_result(Ok(()));
    accepts_alias(Ok(()));
    let error = PulseError::validation("baseline", "baseline");
    assert_eq!(error.code(), "baseline");
    let kernel_error = PulseError::kernel("baseline", "baseline", "a hint");
    assert_eq!(kernel_error.hint(), Some("a hint"));
}

#[test]
fn store_and_kernel_public_paths_compile() {
    let repo = tempfile::tempdir().unwrap();

    let ticket = serde_json::json!({
        "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
        "status": "draft", "revision": 1,
        "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
        "role": "implementation",
    });
    validate_record(&ticket).unwrap();
    issues_mutate(repo.path(), |mut records| {
        records.push(ticket.clone());
        Ok(records)
    })
    .unwrap();
    let records = read_all(repo.path()).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(common_fields(&records[0]).unwrap().id, "TK-a3f9");

    let report: ReadyReport = evaluate_ready(repo.path(), &ticket, &records);
    let _violation: Option<&ReadyViolation> = report.violations.first();

    let human = ActorRef {
        kind: ActorKind::Human,
        id: "quan".to_string(),
    };
    assert!(authorize(&human, Action::MutateGraph).is_ok());
    let _dep_type = DepType::BlockedBy;
    let _note_kind = NoteKind::Note;

    // `source_snapshot` needs a real git repository; this guard only checks
    // that the path and the `Source` shape are reachable, not git behavior
    // (covered in `src/source.rs`'s own tests).
    fn accepts_source_snapshot(_: fn(&std::path::Path, &[String]) -> Result<Source>) {}
    accepts_source_snapshot(source_snapshot);
    let source = Source {
        commit: "0".repeat(40),
        dirty_hash: "sha256:0".to_string(),
        dirty_paths: Vec::new(),
    };
    assert!(source_same(&source, &source));
    assert_eq!(state_repo_root(repo.path()).unwrap(), repo.path());
}
