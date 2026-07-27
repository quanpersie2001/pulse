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
5. CLI examples below describe the product contract for an explicitly enrolled
   target repository or a temporary test fixture. They are not instructions to
   apply those commands to this repository itself.
6. Self-hosting requires an explicit maintainer request and accepted design
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

## Core Principle

The repository is the system of record. Important state, decisions, evidence and durable knowledge must be local, inspectable and recoverable.

Message history is not source-of-truth. Runtime state is not durable docs truth. Work prose is not a substitute for evidence or documentation receipts.

## Architecture Map

```text
Human / Agent
  -> Pulse CLI / kernel
  -> Local Work Graph + Documentation System + Evidence Store
  -> Repository Harness + Source Repository
  -> Verify / Review / QA / Compound Learnings
```

Key planes:

1. **Work graph** — `.pulse/workgraph/nodes/*.json`, `.pulse/workgraph/edges/*.json`, projections/cache.
2. **Work content** — `works/` prose/artifacts owned by Epics, Stories, Tickets and Decisions.
3. **Documentation knowledge** — `docs/`, `AGENTS.md`, future `PULSE.md`, registry and generated navigation.
4. **Evidence** — `.pulse/evidence/` receipts, artifacts and validation bindings.
5. **Runtime** — `.pulse/runtime/` locks, transactions, cache and ephemeral coordination state.
6. **Repository harness** — scripts, tests, skills, policies, evals and verification profiles.

## Current Kernel / CLI Surface

Use the local Rust CLI/kernel, not the legacy `pulse:workflow` skill router,
when implementing or testing Pulse against explicit fixtures/target repositories.
The self-hosting boundary above takes precedence: do not point these commands at
this repository root unless the maintainer explicitly requests it.

Common work graph commands:

```bash
pulse --repo-root <repo> work create --kind ticket --title "..." --json
pulse --repo-root <repo> work show <id> --json
pulse --repo-root <repo> work list --json
pulse --repo-root <repo> work edit <id> --expected-revision <n> ... --json
pulse --repo-root <repo> work transition <id> --to <status> --expected-revision <n> --actor <actor> --json
pulse --repo-root <repo> work supersede <old-id> --by <new-id> --expected-revision <n> --reason "..." --reconciliation-receipt <receipt-id> --actor <actor> --json
pulse --repo-root <repo> work executability <id> --json
pulse --repo-root <repo> work rollup <id> --json
pulse --repo-root <repo> graph export --json
pulse --repo-root <repo> graph validate --json
pulse --repo-root <repo> graph recover --json
pulse --repo-root <repo> graph neighborhood <id> --json
pulse --repo-root <repo> graph affected-by <id> --json
```

Evidence commands:

```bash
pulse --repo-root <repo> evidence artifact put <path> --json
pulse --repo-root <repo> evidence artifact verify <hash> --json
pulse --repo-root <repo> evidence receipt record <receipt-file> --json
pulse --repo-root <repo> evidence receipt verify <receipt-id> --json
pulse --repo-root <repo> evidence receipt show <receipt-id> --json
```

Documentation commands:

```bash
pulse --repo-root <repo> docs register ... --json
pulse --repo-root <repo> docs edit <doc-id> ... --json
pulse --repo-root <repo> docs retire <doc-id> ... --json
pulse --repo-root <repo> docs supersede <old-doc-id> <new-doc-id> ... --json
pulse --repo-root <repo> docs list --json
pulse --repo-root <repo> docs show <doc-id> --json
pulse --repo-root <repo> docs validate --json
pulse --repo-root <repo> docs applicable --work <work-id> --json
pulse --repo-root <repo> docs impact <ticket-id> --expected-revision <n> --posture <required|none|deferred> ... --json
pulse --repo-root <repo> docs index --json
pulse --repo-root <repo> docs index --check --json
pulse --repo-root <repo> docs status --json
pulse --repo-root <repo> docs search "query" [--work <work-id>] [--limit <n>] [--json]
pulse --repo-root <repo> docs get <doc-id|section-ref|chunk-ref|path:start-end> --json
pulse --repo-root <repo> docs tree [path] --json
```

Prefer `--json` for agent consumption. Treat CLI error codes as the stable contract.

## Work Graph Rules

These rules govern Pulse-managed target repositories and test fixtures; they do
not enroll this development repository into Pulse.

- Ticket is the executable unit.
- Epic/Story hold durable outcome, behavior baseline and design/approach context.
- Decisions capture accepted hard-to-reverse choices.
- Hierarchy is not dependency. Use edges/dependencies explicitly.
- Priority is a signal, not an absolute sorting law.
- All mutations use CAS via `expected_revision` and emit immutable events.
- Never manually edit canonical graph JSON unless doing deliberate repair with tests/recovery context.
- Run `graph validate` after broad graph mutations.

## Documentation Rules

Durable repository knowledge belongs in:

- `docs/`
- `AGENTS.md`
- future `PULSE.md`
- accepted Decisions / work artifacts when they own the knowledge

Documentation registry controls identity, lifecycle, authority, scope and retrieval policy.

Rules:

1. Public behavior, invariant, architecture or operator procedure changes must update, classify or defer docs through `docs impact`.
2. Generated `_index.md` navigation files are derived; do not hand-edit generated Pulse markers.
3. Section retrieval uses `docs index/search/get/tree`; do not bypass it by raw-scanning the entire docs tree unless debugging the retrieval system itself.
4. `AGENTS.md` is operator guidance. It must not become a long design doc; link to owning reboot docs.
5. Treat stale/retired/superseded docs as historical, not current truth.

Relevant design owners:

- [`pulse-reboot/10-documentation-system.md`](pulse-reboot/10-documentation-system.md)
- [`pulse-reboot/11-documentation-retrieval.md`](pulse-reboot/11-documentation-retrieval.md)

## Evidence And Verification Rules

- Verification claims should be backed by receipts, test output or committed evidence.
- Receipt validation has integrity, binding, registry/policy and authorization dimensions.
- Authorization may remain explicitly unresolved; do not turn structural checks into human approval.
- Supersession through the CLI is receipt-first: use `--reconciliation-receipt`, not inline `--assertion`.
- Source/content-bound receipts are the durable proof boundary for documentation and review claims.

Relevant owner: [`pulse-reboot/07-verification-ratchet.md`](pulse-reboot/07-verification-ratchet.md).

## Shaping / Readiness Discipline

Pulse is not a fixed phase workflow, but it requires readiness discipline:

- Ground before asking: read work context, applicable docs, decisions, code and evidence.
- Ask humans only across authority boundaries: intent, irreversible trade-offs, risk appetite and approval.
- Turn sharp uncertainty into Decision, Discovery/Spike or enabling Ticket.
- Keep bounded fog in `not_yet_specified`; do not invent speculative tickets upfront.
- If execution discovers critical ambiguity outside implementation freedom, stop and re-shape/requeue.

Owner: [`pulse-reboot/04-runtime-harness.md`](pulse-reboot/04-runtime-harness.md).

## Agent Operating Rules

1. Start by orienting from repository artifacts, not conversation memory.
2. In this Pulse development repository, edit source/design docs and use tests or
   temporary fixtures. Use the Pulse CLI/kernel for canonical reads and
   mutations only in an explicitly enrolled target repository or fixture; never
   infer that this repository is enrolled.
3. Respect CAS revisions in Pulse-managed target repositories; on conflict,
   reload current state before retrying.
4. Keep runtime transactions recoverable; if interrupted, run/read `graph recover` before continuing graph work.
5. Prefer small, evidence-backed commits.
6. Do not mark work complete unless tests/verification prove the affected behavior.
7. Use sub-agents/isolated worktrees for parallelizable investigation or implementation, then verify and merge deliberately.
8. Do not let sub-agent summaries substitute for checking actual diffs and tests.
9. Keep generated/cache/runtime outputs out of durable source unless explicitly tracked by design.
10. When context exceeds about 65%, write a handoff with branch, commits, tests run, open blockers and next action.

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
  `docs`, `graph`, `evidence`, `knowledge`) plus `args` and `output`. It
  translates CLI input/output to library calls and owns no domain semantics.
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

The current domain crates are: `docs`, `evidence`, `graph`, `knowledge`,
`process`, `storage`, `target_repo`. The `process` crate isolates the
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
5. If operating on Pulse work items in an explicitly enrolled target repository,
   update their status or leave a clear handoff. Do not create such items in this
   development repository without explicit self-hosting approval.

## Optional Memory Tools

CASS (`cass`) and cass-memory (`cm`) are optional recall accelerators. Treat current repo artifacts, events and receipts as source-of-truth when discrepancies appear.
