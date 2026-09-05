# Pulse — Architecture (current code)

> Status: written 2026-09 after the narrow-scope code cut ([Decision 0008]).
> This file describes the **code that exists today** and nothing else. Target
> design and product shape live in [`PRODUCT.md`](PRODUCT.md); a deliberate
> difference between the two is a gap to close, not a licence to guess.

## 1. Shape

Pulse is one Rust library plus a thin CLI binary. There is no daemon, no
runner, no agent runtime, no network service. Every command runs against a
target repository on disk, takes a repository-scoped write lock for
mutations, and writes canonical JSON under `.pulse/` with an immutable event
appended to `.pulse/events/` per successful mutation.

The binary (`src/bin/pulse.rs`) only parses, runs and renders errors through
`pulse::cli`. CLI modules (`src/cli/`) are thin transport/renderer adapters
that own no domain semantics. All behaviour lives in the library.

## 2. Layers and dependency direction

Modules sit bottom-up and never reach up the ladder. Guards:
`tests/graph/architecture_guards.rs` scans the source tree, and
`tests/public_api_contract.rs` locks stable public paths by compiling them.

- `src/graph/` is the work-graph domain, layered strictly bottom-up:
  - `graph::model` — pure value types: node, edge, contract, ticket brief,
    lifecycle, manifest. `brief` parses `ticket.md` without I/O or store imports.
  - `graph::validation` — pure semantic validation of model values
    (`contract`, `graph`).
  - `graph::read` — pure snapshot evaluators with no I/O: readiness,
    frontier, rollup, executability, projection and traversal. Ticket
    ambiguity is evaluated from the parsed brief; shaping is not a receipt
    projection.
  - `graph::store` — persistence: sharded JSON node/edge files, manifest,
    bootstrap, repository state classification, docs-impact mutation seam,
    ticket sync from `works/<id>/ticket.md`, supersession. `JsonGraphStore`
    is the public facade.
  There are no one-line re-export shims under `src/graph/`; use the layered
  paths (`graph::model::node` …).
- `src/kernel/` composes graph with other domains into the concrete
  operations: readiness snapshot, lifecycle transitions, packet assembly,
  reservation/lease (`reservation`), handoff/verification/close gates
  (`completion`, `story_completion`), documentation validation
  (`documentation`), and repository init (`init`). It may import docs,
  evidence, policy and qa; nothing imports kernel.
- `src/docs/` is the documentation domain: eight-field model/registry and
  controlled tags (`model`, `registry`, `manifest`, `tags`), markdown section
  extraction (`markdown`, `section`), lexical index and search over
  `.pulse/cache` (`lexical`, `index`, `search`, `cache`, `get`, `tree`),
  path/tag applicability (`applicability`, `policy`), mechanical checks
  (`check`, `projection`, `validate`, `receipt_validation`).
- `src/evidence/` is the immutable proof domain: model/envelope
  (`model`, `manifest`, `receipt/envelope`), kind validators
  (`receipt/{supersession,decision,documentation}`),
  artifact store (`artifact`) and the record/verify store
  (`receipt/store`). QA payload semantics are validated by `src/qa`.
- `src/qa/` is the behavioral-QA contract domain: Story baseline parsing
  (`baseline`), checkpoint payload validation (`receipt`) and the executor
  contract that is the seed of the future runner (`executor`).
- `src/knowledge/` is the ratchet store: model, validate, relation,
  projection and store. Validation and persistence only; no capture/promote
  pipeline yet.
- Shared vocabulary and primitives:
  - `src/identity/` actor kinds/refs (re-exported neutrally by evidence).
  - `src/policy/` default-deny authority policy load/validate/authorize.
  - `src/event.rs` append-only event envelope + ids.
  - `src/source.rs` git source identity (head commit, cleanliness).
  - `src/execution.rs`, `src/reservation.rs`, `src/work_packet.rs`,
    `src/canonical_json.rs` — Core-owned proof/lease/packet contracts.
  - `src/storage/` — generic atomic write, file lock, safe paths and the
    prepared multi-target transaction primitive. Graph bootstrap is
    re-exported through `storage` for compatibility only.

## 3. On-disk planes

Everything is tracked under Git except `.pulse/runtime/` and `.pulse/cache/`
(always disposable). Initialisation is `pulse init`; it preflights every
domain, then creates each plane once.

| Path | Owner | Content |
|---|---|---|
| `docs/`, `works/`, `knowledge/learnings/` | repository | durable prose: documentation, work content, learning narratives |
| `.pulse/workgraph/` | `graph` | `manifest.json`, `nodes/<id>.json`, `edges/<type>--<a>--<b>.json`, `schemas/{node,edge}.schema.json` |
| `.pulse/evidence/` | `evidence` | `manifest.json`, `receipts/<id>.json`, `artifacts/sha256/<hash>` |
| `.pulse/docs/` | `docs` | `registry.json` (eight-field records) and `tags.json` vocabulary |
| `.pulse/knowledge/` | `knowledge` | `manifest.json`, `entries/LRN-*.json`, `relations/` |
| `.pulse/policy/` | `policy` | `authority.json` (default-deny grants) |
| `.pulse/events/` | `event.rs` | `events/<date>/<id>.json` audit trail |
| `.pulse/runtime/` | `storage` | write locks and prepared-transaction intents; gitignored |
| `.pulse/cache/` | `docs` | lexical index generations and pointers; gitignored |

Conventions: canonical JSON bytes everywhere; `revision` is a CAS for node
mutations and `contract_revision` only moves when semantic contract input
changes; evidence receipts are immutable content-hashed envelopes; mutations
are atomic multi-file transactions (intent → prepared → committed) that
recover on crash rather than guessing.

## 4. Test layout

One Cargo integration crate per domain; each `tests/<domain>.rs` wires
`tests/<domain>/*.rs` with `#[path]`, and shared helpers in `tests/common/`
are included per crate. `tests/public_api_contract.rs` is its own crate.

- `tests/graph/` — nodes/edges, lifecycle transitions, readiness/frontier/
  rollup, supersession, reservation/lease and handoff/verification/close
  gates, story close, packet contract, architecture source guards.
- `tests/docs/` — registry, markdown extraction, lexical index/search/get/
  tree, cache concurrency and crash recovery, validation/checks, receipts.
- `tests/evidence/` — receipt envelope integrity, bindings, artifact store,
  kind validators and crash-safe recording.
- `tests/knowledge/` — store, relations and validation boundary.
- `tests/process/` — timing-sensitive subprocess suites for crash/transaction
  recovery.
- `tests/storage/` — generic primitives (atomic, lock, paths, transactions).
- `tests/target_repo/` — fixture copies, `init`, CLI end-to-end enrolment.

Repositories under `tests/fixtures/target-repos/` are immutable inputs;
tests copy them to a temp dir through `TestRepo` and never mutate the fixture
in place. `examples/todolist/` is the tracked dogfood target where Pulse is
run for real (never `--repo-root .` in this repository).

Validation before claiming work done:

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets
```

## 5. Behavioural seams that deliberately have no home yet

These come from `PRODUCT.md`; none is implemented in the tree today, and no
module pretends otherwise.

| Capability | PRODUCT.md | Today |
|---|---|---|
| Runner (`pulse run <role>`) | §5.3 | Implemented spine: `kernel::run` composes lease, run workspace, `runner` mechanics and events; worker handoff/verify CLI exists. Isolation, recovery saga and artifact ingest are still ahead. |
| Ratchet commands | §5.6 | `knowledge` store/validate only; no capture/validate/promote/applicable, no packet injection. |
| Events tail / notes | §5.7 | append-only log exists; `events tail` and `note` CLI do not. |
| MCP server | §5.8 | stub removed with the daemon; CLI path comes first. |
| Ticket close for all risks | §5.5 | Implemented; medium/high/critical use the same proof gates, with a human actor for high/critical. |
| ticket.md as the contract source | §5.1 | Implemented through `graph::model::brief`, `work sync`, ambiguity gating and packet raw content; the legacy JSON contract API was removed. |
| Docs metadata reduction | §5.4 | Implemented: eight-field records, controlled tags and path/tag applicability. |

## 6. Conventions

- `//!` rustdoc on modules with non-obvious invariants; concise `///` on
  public APIs with `# Errors` / `# Panics` sections where relevant.
- Return `Result` for boundary failures; `panic!` only for unrecoverable
  invariants (never for user input or filesystem state).
- Private by default; change stable public paths only deliberately and update
  `tests/public_api_contract.rs` and the architecture guards with the change.
- Preserve atomic-write, lock-ordering and transaction-recovery invariants at
  every storage boundary.

[Decision 0008]: docs/decisions/0008-narrow-scope-to-truth-layer.md
