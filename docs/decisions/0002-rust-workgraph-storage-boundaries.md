# Decision 0002: Rust Workgraph Storage Boundaries

## Status

Accepted for the current Phase 1 experimental harness.

## Context

Phase 1 Slice 1 asked for repository-scoped locking, sharded canonical JSON files,
prepared transaction recovery, JSON Schema-backed contracts, and platform-aware
atomic replace behavior. The Rust implementation currently enforces node/edge
contracts with typed `serde` deserialization plus explicit semantic validators,
and it ships repository-owned JSON Schema templates for drift detection and
bootstrap.

## Decision

- Workgraph reads and read-model projections acquire the same repository
  `WriteGuard` used by mutations, run transaction recovery first, then load and
  validate canonical node/edge files. This is the v1 read-consistency mechanism
  for multi-target supersession; it may serialize readers with writers, but it
  prevents graph-semantic CLI reads from returning a half-applied supersession.
- Runtime JSON Schema validation is not yet implemented. The accepted boundary
  for this slice is: parse with strict typed models (`deny_unknown_fields` where
  present), run explicit semantic validation, and validate repository schema
  files by exact drift comparison against embedded templates/known predecessors.
  A future JSON Schema engine can be added without changing canonical file
  ownership.
- Atomic replace is scoped to one file at a time. On Unix the implementation
  writes a same-directory temp file, fsyncs it, renames over the target, and
  attempts parent-directory fsync. On Windows the implementation documents a
  best-effort boundary: parent directory fsync is not implemented here and
  replace-existing uses remove-then-rename. Multi-file logical mutations rely on
  prepared transaction intents with durable after payloads and roll-forward
  recovery rather than claiming filesystem-level atomicity across files.

## Consequences

- `graph validate`, `graph export`, `work show/list`, executability, rollup,
  neighborhood, and affected-by are slower under concurrent mutation than a
  snapshot-switch design, but they have a concrete consistency contract now.
- Common node/edge schema fixtures still need a runtime JSON Schema validator
  before Pulse can claim full JSON Schema engine coverage; current tests cover
  typed/semantic validation and schema-template drift.
- Platform crash durability remains bounded by local filesystem behavior and the
  preserved `.pulse/runtime/transactions/` directory. The implementation must not
  claim unconditional audit completeness if runtime intent state is lost between
  canonical target replace and event creation.
