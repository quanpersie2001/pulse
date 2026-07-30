# AGENTS.md — Pulse Reboot Operator Contract

Read this file at every session start. Re-read after context compaction.

## What Pulse Is Now

Pulse is a **local-first harness engineering system** for making a repository understandable, executable, verifiable and improvable by coding agents.

Pulse combines:

1. a local work graph for Epics, Stories, Tickets, Decisions and their relations;
2. durable documentation knowledge with ownership, applicability, authority and validation;
3. repository harness capabilities: scripts, tools, hooks, skills, policies and evals;
4. evidence loops for verification, review, QA and receipts;
5. knowledge compounding loops that promote proven learnings into docs, decisions, checks, skills or evals;
6. optional peer-agent orchestration once single-agent reliability is proven.

Pulse is **not** Jira-lite, a fixed phase workflow, a cloud-first service, or a general-purpose agent framework.

Primary design source: [`PULSE_REBOOT.md`](PULSE_REBOOT.md).
Detailed owners live under [`pulse-reboot/`](pulse-reboot/).

## Repository Role And Self-Hosting Boundary

This repository **develops the Pulse harness**. It is not currently enrolled as
a target repository managed by Pulse, and Pulse is not currently self-hosting
its own development work graph here.

Rules:

1. **Do not run Pulse work-graph, evidence, docs-registry or lifecycle mutations
   with `--repo-root .` in this repository.** Do not bootstrap
   `.pulse/workgraph/nodes/`, `.pulse/workgraph/edges/`, `.pulse/events/`,
   `.pulse/evidence/`, `.pulse/docs/` or `works/` as a side effect of planning or
   implementation work.
2. Do not run validation/read commands against `--repo-root .` when they may
   implicitly bootstrap, migrate or rewrite repository state. Use test fixtures,
   temporary repositories and `cargo test` for kernel/CLI validation.
3. Existing legacy paths such as `.pulse/workgraph/items.jsonl`,
   `.pulse/workgraph/schema.json`, local `target/*/pulse` binaries, or other
   development fixtures are **not evidence that self-hosting is active**.
4. For planning Pulse implementation in this repository, use `proposals/`,
   owning `pulse-reboot/` documents, source/tests and Git history. Do not create
   canonical Pulse Story/Ticket/Decision nodes for the work unless the
   maintainer explicitly approves a separate self-hosting migration.
5. Self-hosting requires an explicit maintainer request and accepted design
   decision covering migration, ownership and rollback. Never infer consent from
   the presence of `.pulse/` files.

## Target Repository Test Convention

Pulse integration tests exercise the harness on tracked target-repository
fixtures, never on this development repository and never by mutating a tracked
fixture in place.

Canonical flow:

```text
tests/fixtures/target-repos/<fixture>/   tracked read-only template
                    |
                    | TestRepo::from_fixture(...)
                    v
external TempDir                         mutable working copy + Git baseline
                    |
                    | pulse --repo-root <temp-copy>
                    v
assertions + automatic cleanup
```

Rules:

1. Use `tests/fixtures/target-repos/minimal-service/` as the default realistic
   target repository unless a scenario requires a dedicated fixture.
2. Rust integration tests should use
   `tests/common/fixture_repo.rs::TestRepo::from_fixture`; extend the shared
   helper instead of reimplementing unsafe copy/CLI setup in each test.
3. The tracked fixture is immutable test input. It must not contain generated
   `.pulse/` state or a nested `.git/`, and tests must not run Pulse directly
   against its path.
4. `TestRepo` copies the fixture outside this repository, initializes a clean
   deterministic Git baseline, passes only that path as `--repo-root`, and
   removes it when the `TempDir` is dropped.
5. Tests that need malformed/corrupt state should create it in the temporary
   copy or use a clearly named dedicated negative fixture; never corrupt the
   shared template.
6. Manual smoke tests follow the same rule: copy/create a target under `mktemp`
   and point `target/debug/pulse` there. Never use `--repo-root .`.

## Agent Operating Rules

1. Start by orienting from repository artifacts, not conversation memory.
2. Work on this repository directly through its source, design documents, tests
   and Git history. Do not use Pulse to plan, track, coordinate or execute work
   on Pulse itself.
3. Use the Pulse CLI only when a test explicitly exercises it against a
   temporary target-repository fixture.
4. Prefer small, evidence-backed changes.
5. Do not mark work complete unless tests or focused verification prove the
   affected behavior.
6. Keep generated, cache and runtime outputs out of durable source unless the
   repository design explicitly tracks them.
7. When handing work off, record the branch, relevant changes, tests run, open
   blockers and next action.

## Repository Layout Quick Reference

The `.pulse/workgraph/nodes`, `.pulse/workgraph/edges`, `.pulse/events` and
`works/` paths below are the **designed target-repository layout**, not proof that
this development repository should contain or bootstrap them. Paths that exist
here may be schemas, fixtures or legacy development artifacts.

```text
.pulse/workgraph/nodes/          target-repo canonical work nodes
.pulse/workgraph/edges/          target-repo canonical graph edges
.pulse/runtime/                  target-repo ephemeral coordination state
.pulse/evidence/                 target-repo receipts/artifacts or test fixtures
.pulse/docs/                     target-repo registry/schemas or test fixtures
.pulse/cache/                    disposable generated caches
works/                           target-repo work prose; absent here until self-hosting is approved
docs/                            durable repository documentation
pulse-reboot/                    reboot design owner documents
proposals/                       Pulse implementation slice proposals
src/                             Rust Pulse kernel/CLI implementation
tests/                           integration and contract tests
```

## Source Architecture

The `src/` tree is organized by ownership and dependency direction. Pure/value
layers sit below persistence and cross-domain composition. Put new code in the
layer that matches its responsibility and do not reach up the dependency ladder.
These seams are guarded by `tests/graph/architecture_guards.rs` and
`tests/public_api_contract.rs`; those tests are the durable guard, this section
is orientation.

- `src/bin/pulse.rs` is a minimal adapter (parse, run, render error) that
  delegates entirely to the `pulse::cli` facade. It wires no domains and must
  not include production modules with `#[path]`.
- `src/cli/` is thin transport/renderer grouped by command domain (`work`,
  `docs`, `graph`, `evidence`, `knowledge`, `daemon`) plus `args` and `output`.
  Offline Core commands call library services; runtime commands use the local
  daemon protocol. CLI owns no provider or domain semantics.
- `src/daemon/` is the sole runtime lifecycle authority. Its application layer
  composes daemon-owned Project/Workspace/Session/Provider/ProcessOwner/
  timeline state with narrow public Core reservation and proof APIs. Local and
  MCP transports share the same versioned envelopes, authorization and
  idempotency behavior. Core modules must never import `daemon`.
- `src/kernel/` is the concrete cross-domain composition layer (no trait
  abstractions): it assembles typed inputs from the graph store, documentation,
  evidence, policy and source checks to build readiness, shaping, lifecycle-gate
  and frontier snapshots. Cross-domain orchestration that combines domains lives
  here; the graph store's own cross-domain surface stays narrow (evidence
  receipt binding in supersession, authority-gated contract writes).
- `src/graph/` is layered, bottom-up:
  - `model/` — pure value types (node, edge, contract, lifecycle, manifest);
    depends only on identity/event/canonical-json, never on store/readiness.
  - `validation/` — contract and whole-graph validators; depends on model, not
    on store.
  - `read/` — pure snapshot-in/snapshot-out evaluators (readiness,
    executability, frontier, rollup, traversal, shaping, projection); no
    filesystem or store imports.
  - `store/` — `JsonGraphStore` persistence and mutation (nodes, edges,
    contracts, supersession, repository, bootstrap). Read-only entrypoints stay
    thin and delegate to the pure evaluators in `read/`.
  The historical single-file modules (`src/graph/<name>.rs`) survive only as
  one-line compatibility re-exports; use the layered paths for new code.
- `src/evidence/` owns immutable receipt envelope integrity, generic bindings,
  recording and kind dispatch. `receipt/` is split into focused validators
  (`envelope`, `bindings`, `supersession`, `shaping`, `decision`,
  `documentation`, `store`, `helpers`). Documentation receipt *policy*
  interpretation (registry lifecycle, review posture) is owned by
  `src/docs/receipt_validation.rs` and consumed by evidence as a narrow
  validator; evidence does not implement docs policy.
- `src/identity/` owns the shared actor vocabulary (`ActorRef`, `ActorKind`)
  used by evidence, event, policy and kernel. `evidence::model` re-exports it
  for path compatibility.
- `src/storage/` owns only generic primitives (atomic write, locking, path
  validation, transactions). Workgraph bootstrap and embedded schema templates
  are owned by `graph::store::bootstrap` and re-exported through `storage` for
  compatibility; generic storage must not depend on the graph domain.
- `src/source.rs` owns source-binding currentness and intentionally couples git
  mechanics with status policy. A physical move to a `repository/` namespace
  was reviewed and deferred; the public `pulse::source` path is the contract.

## Validation Commands For This Repo

Before claiming implementation work is done, run:

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets
```

`cargo test --all-targets` is the reliability bar: it must pass under its
default threading. Never work around a flaky suite by lowering `--test-threads`;
fix the underlying race, failpoint synchronization or global-state collision.

Optional retrieval benchmark smoke (not covered by `--all-targets`):

```bash
cargo bench --bench docs_retrieval -- --smoke
```

### Test layout convention

Integration tests are organized as one Cargo integration crate per domain:

- `tests/<domain>.rs` — the only top-level entry file per domain. It is the
  crate root and wires its submodules explicitly with `#[path = "..."]`.
- `tests/<domain>/` — the submodules for that crate (e.g. `tests/graph/`).
  There is no bare `tests/<name>.rs` per coverage area; run a single area by
  passing its module name as a filter to the domain crate.
- `tests/common/` — shared mechanical helpers (CLI binary resolver, git
  plumbing, canonical-JSON writer, `fixture_repo::TestRepo`). Each crate
  includes only the helper files it uses via `#[path = "common/<file>.rs"]`,
  because a helper compiled into one integration crate is invisible to the
  others and unused helpers trip `dead_code` under `-D warnings`.
- `tests/fixtures/target-repos/minimal-service/` — the immutable, read-only
  target-repository template. Tests copy it out of repo via `TestRepo`; never
  run Pulse against it in place and never commit generated `.pulse/` state.
- No legacy Node/`.test.mjs` behavioral runners remain in the active test tree
  (they were removed during the reboot). Do not reintroduce them.

The current domain crates are: `daemon`, `docs`, `evidence`, `graph`,
`knowledge`, `process`, `storage`, `target_repo`. The `process` crate isolates the
subprocess-spawning, timing-sensitive suites (multi-process CAS, failpoint
crash recovery, supersession process recovery) so per-crate parallelism stays
bounded; keep timing-sensitive suites there rather than in `graph`.

### Focused suites

Target a coverage area by running its domain crate with a module-name filter:

```bash
# Docs retrieval / index / search
cargo test --test docs -- docs_search_get_tree
cargo test --test docs -- docs_index
cargo test --test docs -- docs_retrieval_eval

# Evidence / receipts / crash recovery
cargo test --test evidence -- evidence_receipts
cargo test --test docs -- docs_receipt_registry
cargo test --test process -- crash_recovery_process

# Graph / lifecycle
cargo test --test graph -- lifecycle
cargo test --test graph -- workgraph
cargo test --test graph -- workgraph_transaction
cargo test --test graph -- workgraph_read_models

# Process / concurrency / recovery (timing-sensitive suite)
cargo test --test process
cargo test --test knowledge -- knowledge_crash_recovery_process
cargo test --test knowledge -- knowledge_process_concurrency

# Storage recovery primitives
cargo test --test storage -- transaction_recovery
```

## Session Completion

Before ending a substantial work chunk:

1. Ensure working tree state is intentional.
2. Run the relevant validation commands and record them in the final response.
3. Commit coherent changes.
4. Note branch, commits, remaining risks and next action.

## Optional Memory Tools

CASS (`cass`) and cass-memory (`cm`) are optional recall accelerators. Treat current repo artifacts, events and receipts as source-of-truth when discrepancies appear.
