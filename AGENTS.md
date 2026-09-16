# AGENTS.md — Pulse Operator Contract

Read this file at every session start. Re-read after context compaction.

## What Pulse is

Pulse is a local CLI truth layer for a developer using coding agents in one
repository: a JSONL store of Epic/Story/Ticket/Decision records, a runner
that dispatches configured worker/review/qa roles, an evidence gate, and an
append-only event log. It does not run agents, does not run tests, and has
no daemon.

Pulse is mid-rewrite. **[`docs/plans/0022-thin-harness.md`](docs/plans/0022-thin-harness.md)
(plan 0022) is the current design and implementation plan; when it
disagrees with `PRODUCT.md`, `ARCHITECTURE.md`, `ROADMAP.md` or any
decision before [Decision 0022](docs/decisions/0022-thin-harness.md), plan
0022 wins.** Those older docs describe v2 and stay historical until
`SPEC.md` (plan 0022 Phase 3) replaces `PRODUCT.md`.

## Repository roles

- This repository **develops** Pulse. Never run Pulse mutations with
  `--repo-root .` here.
- **There is currently no dogfood target.** Plan 0022 Phase 2 creates one at
  `~/Workspace/Personal/todolist`, outside this repository. Until then,
  Pulse runs for real nowhere — see plan 0022 §2 before adding features.
- `tests/fixtures/target-repos/<fixture>/` are **immutable test inputs**.
  Tests copy them out through `tests/common/fixture_repo.rs::TestRepo` and
  run Pulse against the temporary copy. Never run Pulse against a fixture
  in place and never commit generated `.pulse/` state into one.

## Agent operating rules

1. Orient from repository artifacts (plan 0022, source, tests, decisions,
   Git history), not conversation memory.
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

Layers sit bottom-up; never reach up the ladder. Guarded by
`tests/architecture_guards.rs` and `tests/public_api_contract.rs`.

- `src/bin/pulse.rs`: parse, run, render error; delegates to `pulse::cli`.
- `src/cli/`: thin transport/renderer (`args`, `work`, `run`, `packet`,
  `completion`, `events`, `init`, `output`). Owns no domain semantics.
- `src/kernel/`: cross-domain composition — `issues` (new/update/dep/
  transition/note), `ready` (draft -> ready gate), `roles` (actor
  authorization matrix), `reservation` (lease), `run` (worker continue
  loop + lane spawn), `lane` (lane input/output/seal), `completion`
  (handoff/close/close-story gates), `packet`, `checkpoint`, `profile`
  (`PULSE.md`), `init` (repository enrollment).
- `src/store/issues.rs`: the one JSONL store (`.pulse/issues.jsonl`),
  embedded-schema validation, atomic read-validate-write.
- `src/storage/`: atomic write, lock, append-fsync, transaction and path
  safety primitives. Generic; no domain knowledge.
- `src/evidence/`: one receipt family (`receipt.rs`), artifact hashing,
  redaction.
- `src/runner/`: process-execution contract — argv split (never a shell),
  placeholders, timeout, bounded output, final-JSON-line contract. No
  graph truth.
- `src/identity/`, `src/event.rs`, `src/source.rs`: actor vocabulary,
  append-only event log, git source fence.

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
`architecture_guards`, `communication`, `golden_path`, `public_api_contract`,
`run`, `runner`, `storage`, `target_repo`.

## Session completion

1. Working tree state is intentional.
2. Relevant validation commands ran; record them in the final response.
3. Commit coherent changes only when asked.
4. Note branch, commits, remaining risks and next action.
