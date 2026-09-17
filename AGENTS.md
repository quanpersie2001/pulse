# AGENTS.md — Pulse Operator Contract

Read this file at every session start. Re-read after context compaction.

## What Pulse is

Pulse is a local CLI truth layer for a developer using coding agents in one
repository: a JSONL store of Epic/Story/Ticket/Decision records, a runner
that dispatches configured worker/review/qa roles, an evidence gate, and an
append-only event log. It does not run agents, does not run tests, and has
no daemon.

**v3 is shipped (tag `v0.0.1` — the rebuild restarts the version line).** [`SPEC.md`](SPEC.md) describes what runs;
[`ARCHITECTURE.md`](ARCHITECTURE.md) describes the code tree; plan
[`docs/plans/0022-thin-harness.md`](docs/plans/0022-thin-harness.md) and its
decisions hold the reasoning. `PRODUCT.md` is v2 history — do not implement
from it.

## Repository roles

- This repository **develops** Pulse. Never run Pulse mutations with
  `--repo-root .` here.
- The **dogfood target** is `~/Workspace/Personal/todolist` (outside this
  repository, registered in `~/.pulse/projects.json`). Every real run goes
  through it; friction lands there as `pulse note <id> "…" --friction`.
- `tests/fixtures/target-repos/<fixture>/` are **immutable test inputs**.
  Tests copy them out through `tests/common/fixture_repo.rs::TestRepo` and
  run Pulse against the temporary copy. Never run Pulse against a fixture
  in place and never commit generated `.pulse/` state into one.

## Agent operating rules

1. Orient from repository artifacts (`SPEC.md`, plan 0022, source, tests,
   decisions, Git history), not conversation memory.
2. Prefer small, evidence-backed changes. Do not mark work complete unless
   tests or focused verification prove the affected behavior.
3. Keep generated, cache and runtime outputs (`.pulse/runtime/`,
   `.pulse/cache/`) out of durable source.
4. When handing off, record branch, changes, tests run, blockers and next
   action.

## Rust standards

Modules make ownership and privacy legible: private by default, smallest
public surface, stable public paths changed only deliberately. Modules with
non-obvious invariants start with `//!` rustdoc stating purpose, state
touched, invariant and allowed dependencies. Public APIs use concise `///`
rustdoc with literal `# Errors` / `# Panics` sections when relevant.

Return `Result` for recoverable or boundary failures. `panic!` only for
genuinely unrecoverable invariants, never for user input, filesystem state
or process outcomes. Every `kernel`/`runner` error code carries a hint
through `PulseError::kernel(code, message, hint)` (plan 0022 §6 — a code
with no hint is a bug).

## Source architecture

`templates/` is everything `pulse init` writes into a target repo,
embedded with `include_str!`; `assets/` is this repository's own media
only.

Layers sit bottom-up; never reach up the ladder (see
[`ARCHITECTURE.md`](ARCHITECTURE.md) for the full map). Guarded by
`tests/architecture_guards.rs` and `tests/public_api_contract.rs`.

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

`tests/*.rs` are flat integration-test crate roots — no per-domain
subdirectory nesting; a crate needing more than one file wires them with
`#[path]` (e.g. `tests/storage.rs` -> `tests/storage/storage_primitives.rs`,
`tests/target_repo.rs` -> `tests/target_repo/*.rs`). Shared helpers live in
`tests/common/` and are included per crate with `#[path]`. Current crates:
`architecture_guards`, `communication`, `doctor`, `golden_path`,
`public_api_contract`, `run`, `runner`, `storage`, `serve`, `target_repo`.

## Session completion

1. Working tree state is intentional.
2. Relevant validation commands ran; record them in the final response.
3. Commit coherent changes only when asked.
4. Note branch, commits, remaining risks and next action.
