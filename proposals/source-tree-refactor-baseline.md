# Source-tree refactor non-regression baseline

Status: implemented/verified record for the source-tree refactor (tasks 48-55).
This is not a Pulse work graph or schema contract; it documents source
ownership, the public Rust path surface that must keep compiling, and the guards
that lock the seams. See [`AGENTS.md`](../AGENTS.md) "Source Architecture" for
the operator-facing summary.

## Purpose

Record the implemented `src/` ownership boundaries, the public Rust path surface
that integration tests and the binary rely on, and the architectural seams the
guards lock. Tests are the durable guard; this document is the orientation and
verification record.

## Implemented source ownership (verified)

The refactor landed the following ownership boundaries. Each is locked by the
guards in `tests/graph/architecture_guards.rs` / `tests/public_api_contract.rs`
and described operator-facing in `AGENTS.md` "Source Architecture".

- **CLI is thin transport.** `src/bin/pulse.rs` is a ~10-line adapter delegating
  to the `pulse::cli` facade; `src/cli/` is grouped by command domain
  (`work`, `docs`, `graph`, `evidence`, `knowledge`) plus `args`/`output` and
  owns no domain semantics. No `#[path]` includes of production modules.
- **Kernel owns cross-domain composition.** `src/kernel/` (`readiness`,
  `shaping`, `lifecycle`, `frontier`) is concrete — no trait abstractions — and
  composes graph store + docs + evidence + policy + source into readiness,
  shaping, lifecycle-gate and frontier snapshots. The graph store no longer
  performs this orchestration.
- **Graph is layered.** `src/graph/{model,validation,read,store}` with strict
  dependency direction `model <- validation <- read <- store`. `model/` never
  depends on store/readiness; `read/` evaluators are snapshot-in/snapshot-out
  with no filesystem/store imports; `store/` read entrypoints delegate to
  `read/`. `src/graph/<name>.rs` survive as one-line compatibility re-exports.
- **Evidence receipt validators are modular.** `src/evidence/receipt/` splits
  envelope/binding/kind validation into focused submodules. Documentation
  receipt *policy* interpretation (registry lifecycle, review posture) lives in
  `src/docs/receipt_validation.rs` and is consumed by evidence as a narrow
  validator; evidence assembles dimensions but implements no docs policy.
- **Identity is neutral.** `src/identity/actor` owns `ActorRef`/`ActorKind`;
  `evidence::model` re-exports them for path compatibility.
- **Storage primitives vs graph bootstrap.** `src/storage/` owns only
  atomic/lock/path/transaction primitives and must not depend on graph.
  Workgraph bootstrap and schema templates are owned by
  `graph::store::bootstrap` and re-exported through `storage` for compatibility.
- **`source.rs` move deferred.** Reviewed and intentionally kept at
  `src/source.rs`; git-mechanics/status-policy coupling is not safely separable
  without duplicating plumbing. The public `pulse::source` path is the contract.

## Public Rust paths inventoried

### Root and graph

- `pulse::JsonGraphStore` and `pulse::graph::store::JsonGraphStore` remain the public graph-store entrypoint used by integration tests and `src/bin/pulse.rs`.
- Store request/result types currently imported through `pulse::graph::store::*` include `OperationContext`, `ContractSetRequest`, `QaImpactUpdate`, supersession request/target/assertion types and mutation/list outcomes.
- Graph model/contract paths used by tests and the binary remain under:
  - `pulse::graph::node::{Node, NodeStatus, DocumentationImpactPosture, ...}`
  - `pulse::graph::edge::{Edge, EdgeType, deterministic_edge_id, ...}`
  - `pulse::graph::contract::{TicketRole, Risk, Materialization, ImplementationContract, DecisionWorkContract, ...}`
  - `pulse::graph::readiness::{ReadinessReport, READINESS_PROFILE, ...}`
  - `pulse::graph::frontier::{FrontierKind, FRONTIER_CLAIM_STATE, ...}`
  - `pulse::graph::lifecycle::TransitionReason`
- Identity/event paths remain `pulse::id::{WorkKind, WorkId, format_id, ...}` and `pulse::event::{EventEnvelope, EventActor, EventSubject, EventCorrelation, ...}`.

### Documentation, evidence, knowledge and storage

- Docs public API is intentionally broad today through `pulse::docs::*`, including registry/model types, index/search/get/tree/applicability option/report types, constants such as `DOCUMENT_SCHEMA`, and registry/index entrypoint functions.
- Evidence exports used by graph/docs/process tests remain under `pulse::evidence::{bootstrap, record_receipt, verify_receipt, put_artifact, ...}` and `pulse::evidence::model::*` for typed receipts, bindings and shaping/decision/documentation payloads.
- Knowledge exports remain under `pulse::knowledge::*`, with store access via both `pulse::knowledge::KnowledgeStore` and `pulse::knowledge::store::{KnowledgeStore, OperationContext, RelationAdd}`.
- Storage interfaces used externally remain `pulse::storage::{bootstrap, safe_repo_relative, MANIFEST_JSON}`, `pulse::storage::atomic::atomic_replace`, `pulse::storage::paths::*`, and `pulse::storage::transaction::*`.

## Guards added

- `tests/graph/public_api_paths.rs` compiles and lightly exercises graph/store/model/contract paths that current graph tests and the binary rely on.
- `tests/public_api_contract.rs` is a cross-domain compile-time compatibility crate for docs, evidence, knowledge, event/identity, storage and root exports.
- `tests/graph/architecture_guards.rs` adds narrow source scans for:
  - CLI binary remains a thin adapter over public library paths and does not path-include production modules.
  - Graph pure evaluators (`contract`, `executability`, `frontier`, `readiness`, `rollup`, `traversal`) do not import filesystem, storage or graph store modules.
  - Graph model/contract layer does not depend on store/readiness.
  - Read-only graph entrypoints remain present on `JsonGraphStore` and continue delegating to pure evaluators.

## Constraints applied during the refactor

The source-tree moves honored these constraints (the refactor is complete; this
list is the record, not a pending plan):

1. Move internal production files behind compatibility re-exports first; keep the public paths above compiling until an explicit public API change is approved.
2. Preserve `src/bin/pulse.rs` as a consumer of library APIs. If CLI helpers move, move them behind library functions or private binary-local helpers, not by including source files with `#[path]`.
3. Keep pure graph evaluators snapshot-in/snapshot-out. Store/repository locking, filesystem reads, cache and receipt loading belong in store or domain services that assemble typed inputs.
4. Keep model/contract types below store/readiness in the dependency direction. Readiness may consume model/contract; model/contract must not consume readiness/store.
5. Do not add schema/version/migration behavior as part of source-tree moves. If a move exposes a needed public API change, update this note and tests in the same task.
6. Use existing CLI integration tests as the binary behavior guard; focused API/architecture guards here are only compile/source-bound non-regression coverage.

## Relocations landed in task #53 (neutral identity and storage layering)

No public Rust path, persisted schema, event payload or version changed. These
moves only relocate internal ownership and add compatibility re-exports:

- **Neutral identity ownership.** `ActorRef` / `ActorKind` now live in
  `pulse::identity::actor` (new `src/identity/`). `pulse::evidence::model::*`
  re-exports them, so the historical receipt/test path is unchanged. Policy and
  kernel import the neutral owner directly.
- **ID generation ownership.** `new_event_id` is defined in `pulse::event`;
  `new_transaction_id` is defined in `pulse::storage::transaction`. Work/learning
  identity (`WorkKind`, `WorkId`, `format_id`, `parse_numeric`, validators) stays
  in `pulse::id`, which also keeps compatibility re-exports of the two relocated
  generators. The dead `edge_id` helper was removed (no callers; the live edge-id
  function is `graph::edge::deterministic_edge_id`).
- **Workgraph bootstrap/schema ownership.** `bootstrap`, `BootstrapOutcome`,
  `MANIFEST_JSON`, `NODE_SCHEMA_JSON`, `EDGE_SCHEMA_JSON` and
  `default_manifest_value` moved to `pulse::graph::store::bootstrap`. Generic
  `pulse::storage` now holds only atomic/lock/path/transaction primitives and
  re-exports the bootstrap surface through the `graph::store` facade for
  compatibility. This removes the previous `storage -> graph::manifest` layering
  inversion.
- **Source snapshot ownership.** Reviewed `src/source.rs`; the physical move to a
  `repository/source_snapshot` namespace was deferred — the module is already
  cohesive and the git-mechanics/status-policy coupling is not safely separable
  without duplicating plumbing. The public `pulse::source::*` path is the contract.
- **Lock contract.** `.pulse/runtime/locks/workgraph.lock` and `WriteGuard` are
  intentionally unchanged; no lock concept was renamed.

Guards added in `tests/graph/architecture_guards.rs` lock the neutral identity
owner, the event/transaction ID-generation owners, the storage generic-layer
isolation from graph, and the graph-owned bootstrap; `tests/public_api_contract.rs`
locks the new `pulse::identity::actor` path and the `pulse::id` compat re-exports.

## Validation snapshot

Evidence snapshot, not a product contract — counts drift as tests are added:

- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean;
  `cargo test --all-targets` is the reliability bar.
- 356 tests across 7 domain integration crates (`docs`, `evidence`, `graph`,
  `knowledge`, `process`, `storage`, `target_repo` — one top-level
  `tests/<domain>.rs` entry each, submodules wired with `#[path]`), plus the
  cross-domain `tests/public_api_contract.rs` compile-time contract crate and
  `src/lib.rs` unit tests. Count is a snapshot, not a target.
- The refactor changed only source/module ownership and re-exports; no public
  Rust path, persisted schema, event payload, lock contract or CLI behavior
  changed. Timing-sensitive crash-recovery suites occasionally flake under
  default parallel threading and pass on re-run or in isolation.
