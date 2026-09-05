# Agent Guidance

## Repository purpose

A minimal todo-list app. It exists as the real target repository the Pulse
harness dogfoods: every feature Pulse claims is exercised here first.

## Source map

- `src/todolist.mjs` owns todo domain logic (pure functions, no I/O).
- `src/cli.mjs` owns the command-line entry point and state file handling.
- `test/todolist.test.mjs` owns executable behavior examples (`node --test`).
- `docs/product/todolist.md` owns the user-visible behavior contract.
- `scripts/verify.mjs` is the deterministic verification entry point.
- `scripts/qa-run.mjs` is the QA runner role script (reads the run input
  contract, records a `qa_checkpoint` receipt, prints one final JSON line).

## Constraints

- Preserve stable public outcome names (`Completed`, `NotFound`) once
  introduced; renaming them is a human-gated decision.
- Do not add third-party runtime or test dependencies.
- Update `docs/product/todolist.md` when public behavior changes.
- Run `node scripts/verify.mjs` before claiming implementation success.
