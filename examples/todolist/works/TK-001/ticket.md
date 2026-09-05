# TK-001 Implement reliable todo completion

## Objective
Add `completeTodo` to the todolist domain module with stable outcome names
(`Completed`, `NotFound`) so callers can complete todos without exceptions,
and expose it through the CLI `done` command.

## Current behavior
There is no way to complete a todo: the module has no `completeTodo`, the
CLI has no `done` command, and `docs/product/todolist.md` documents
completion as "not yet available".

## Target behavior
- `completeTodo(todos, id)` returns `{ outcome: "Completed", todos }` where
  the returned list marks the matched todo done and the input list is
  unchanged.
- Unknown ids return `{ outcome: "NotFound", todos }` with the list
  unchanged.
- `node src/cli.mjs done <id>` completes the todo in the state file and
  prints the outcome name.
- `docs/product/todolist.md` documents the completion behavior in place of
  the current placeholder.

## Code anchors
- src/todolist.mjs
- src/cli.mjs
- test/todolist.test.mjs
- docs/product/todolist.md

## Required changes
- Add `CompleteOutcome` and `completeTodo` to `src/todolist.mjs`.
- Wire a `done` command in `src/cli.mjs`.
- Add unit tests covering both outcomes and input immutability.
- Replace the "Completing todos" section in `docs/product/todolist.md`.

## Invariants
- Functions never mutate their `todos` argument.
- Outcome names `Completed` and `NotFound` are stable public contract.
- No new dependencies.

## Implementation freedom
guided: agent chooses internal structure; outcome names and the
`{outcome, todos}` result shape are fixed.

## Scope
- Domain function, CLI command, tests, behavior doc.

## Non-scope
- Persistence format changes and anything beyond completing todos.

## Acceptance
- AC-1: Completing an existing todo returns Completed and marks it done
  without mutating the input list.
- AC-2: Completing an unknown id returns NotFound and leaves the list
  unchanged.

## Verify
- node scripts/verify.mjs

## Open questions
- (resolved) Result shape is an object `{outcome, todos}` so future metadata does not break callers.
- (delegated) Internal helper structure: worker chooses.

## Documentation impact
- Posture: required
- Documents: DOC-TODOLIST-BEHAVIOR
- Required update: replace the "Completing todos" placeholder section.

## QA impact
- Owner: ST-001
- Posture: required
- Cases: QA-001, QA-002
- Reason: new public behavior on the core list identity invariants.

## Expected handoff
- Diff, `node scripts/verify.mjs` result, AC to check mapping, updated
  behavior doc, docs finding if any.
