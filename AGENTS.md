# AGENTS.md — Pulse Operator Contract

Read this file at every session start. Re-read after context compaction.

## What Pulse is

Pulse is a local CLI truth layer for a developer using coding agents in one
repository: work graph, packet, runner, docs, evidence gate, ratchet,
event-log communication. It does not run agents, does not run tests, and has
no daemon.

Product definition and target design: [`PRODUCT.md`](PRODUCT.md). Current
code architecture: [`ARCHITECTURE.md`](ARCHITECTURE.md). Scope decision:
[Decision 0008](docs/decisions/0008-narrow-scope-to-truth-layer.md).
When this file, README or archived material disagrees with `PRODUCT.md`,
`PRODUCT.md` wins.

Do not add features until the golden path in `PRODUCT.md` §7 runs for real.

## Repository roles

- This repository **develops** Pulse. Never run Pulse mutations with
  `--repo-root .` here. The legacy Node-era `.pulse/workgraph/items.jsonl`,
  `schema.json` and `.pulse/docs/retrieval-evals/` at the root were deleted;
  a tracked `.pulse/` at the root is not evidence of self-hosting.
- `examples/todolist/` is the **dogfood target**: a small app inside this Git
  repository where Pulse is run for real. Its `.pulse/` and `works/` are
  tracked; its `runtime/` and `cache/` are ignored. Run Pulse there with
  `--repo-root examples/todolist`.
- `tests/fixtures/target-repos/<fixture>/` are **immutable test inputs**.
  Tests copy them out through `tests/common/fixture_repo.rs::TestRepo` and run
  Pulse against the temporary copy. Never run Pulse against a fixture in place
  and never commit generated `.pulse/` state into one.

## Agent operating rules

1. Orient from repository artifacts (`PRODUCT.md`, source, tests, decisions,
   Git history), not conversation memory.
2. Prefer small, evidence-backed changes. Do not mark work complete unless
   tests or focused verification prove the affected behavior.
3. Keep generated, cache and runtime outputs out of durable source.
4. When handing off, record branch, changes, tests run, blockers and next
   action.

## Rust standards

Modules make ownership and privacy legible: private by default, smallest
public surface, stable public paths changed only deliberately. Modules with
non-obvious invariants start with `//!` rustdoc stating purpose, state
touched, invariant and allowed dependencies. Public APIs use concise `///`
rustdoc with literal `# Errors` / `# Panics` sections when relevant.

Return `Result` for recoverable or boundary failures. `panic!` only for
genuinely unrecoverable invariants, never for user input, filesystem state or
process outcomes. Clippy's `too_many_lines` is a review signal, not a quota;
decompose by responsibility, not line count.

## Source architecture

Layers sit bottom-up; never reach up the ladder. Guarded by
`tests/graph/architecture_guards.rs` and `tests/public_api_contract.rs`.

- `src/bin/pulse.rs`: parse, run, render error; delegates to `pulse::cli`.
- `src/cli/`: thin transport/renderer per command domain. Owns no domain
  semantics.
- `src/kernel/`: concrete cross-domain composition (readiness, lifecycle,
  packet, reservation, completion, story completion, documentation, init).
  Ticket ambiguity is parsed by `graph::model::brief`; shaping is not a
  separate receipt ceremony. No trait abstractions.
- `src/graph/`: `model/` (pure values) → `validation/` → `read/` (pure
  snapshot evaluators, no I/O) → `store/` (persistence, CAS, supersession,
  bootstrap). Only layered paths exist; do not re-add one-line re-export
  shims under `src/graph/`.
- `src/docs/`: eight-field registry, controlled tags, applicability, markdown
  section extraction, tantivy index, search/get/tree, validation and checks,
  receipt policy.
- `src/evidence/`: immutable receipt envelope, bindings, store, kind
  validators. Docs receipt policy lives in `src/docs/receipt_validation.rs`.
- `src/qa/`: Story baseline parsing, checkpoint receipt semantics, executor
  contract. The command-execution contract here is the seed of the future
  `runner/`.
- `src/knowledge/`: learning store, relations, validation.
- `src/identity/`, `src/policy/`, `src/event.rs`, `src/source.rs`,
  `src/storage/`: actor vocabulary, default-deny authority, append-only event
  log, git source identity, atomic/lock/transaction primitives.

## Validation commands

Before claiming implementation work is done:

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets
```

`cargo test --all-targets` must pass at default threading. Never lower
`--test-threads` to hide a race; fix the race.

### Test layout

One Cargo integration crate per domain: `tests/<domain>.rs` is the crate root
and wires `tests/<domain>/*.rs` with `#[path]`. Shared helpers live in
`tests/common/` and are included per crate with `#[path]`. Current crates:
`docs`, `evidence`, `graph`, `knowledge`, `process`, `storage`,
`target_repo`, plus `tests/public_api_contract.rs` as its own crate.
Timing-sensitive subprocess suites stay in `process`.

Focused runs:

```bash
cargo test --test graph -- lifecycle
cargo test --test graph -- workgraph
cargo test --test docs -- docs_search_get_tree
cargo test --test evidence -- evidence_receipts
cargo test --test process
cargo test --test storage -- transaction_recovery
```

## Session completion

1. Working tree state is intentional.
2. Relevant validation commands ran; record them in the final response.
3. Commit coherent changes only when asked.
4. Note branch, commits, remaining risks and next action.
