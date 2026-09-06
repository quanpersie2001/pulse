# TK-007 Rename a todo in place

## Objective
Let users fix a typo without re-creating a todo: `node src/cli.mjs rename
<id> <new title>` changes the title, keeping id, done state, and position.

## Current behavior
There is no rename; users `remove` and `add`, which loses done state and
position, and changes the id if they pick a new one.

## Target behavior
- `renameTodo(todos, id, title)` returns `{ outcome, todos }` with stable
  public outcome names `Renamed` and `NotFound`, mirroring the
  `completeTodo` shape. The matched todo gets the trimmed new title; id,
  done, position, and any other fields are untouched; the input list is
  never mutated.
- A blank title throws `TypeError` exactly like `createTodo`.
- `node src/cli.mjs rename <id> <new title>` prints the outcome name and
  exits 0 on `Renamed`; on `NotFound` prints `NotFound` and exits 1 with
  the state file untouched — same contract as `done`.

## Code anchors
- src/todolist.mjs
- src/cli.mjs
- test/todolist.test.mjs
- docs/product/todolist.md

## Required changes
- Add `RenameOutcome`/`renameTodo` to the domain module following the
  `CompleteOutcome` pattern.
- Wire the `rename` command in the CLI.
- Domain tests: success path, unknown id, blank title, immutability,
  field preservation (done and position survive).
- Document the command and outcome names in the behavior doc.

## Invariants
- Outcome names `Renamed` and `NotFound` become stable public contract
  once introduced (same human-gated rule as `Completed`/`NotFound`).
- Domain functions never mutate their inputs.
- No new dependencies.

## Implementation freedom
guided: the `{outcome, todos}` result shape and outcome names are fixed;
internal structure is the worker's.

## Scope
- Domain function, CLI command, tests, behavior doc.

## Non-scope
- Renaming ids, batch rename, editing any field other than the title.

## Acceptance
- AC-1: Renaming an existing todo reports `Renamed`, stores the trimmed
  title, preserves id/done/position, exits 0.
- AC-2: Renaming an unknown id reports `NotFound`, leaves the list and
  state file unchanged, exits 1.
- AC-3: A blank title throws `TypeError`; the input list is never mutated
  in any outcome.

## Verify
- node scripts/verify.mjs

## Open questions
- (resolved) Outcome-object shape mirrors `completeTodo` so callers handle both with one pattern.

## Documentation impact
- Posture: required
- Documents: DOC-TODOLIST-BEHAVIOR
- Required update: add rename to "Managing todos" with outcome names.

## QA impact
- Posture: none
- Rationale: domain mutation contract pinned by focused verify tests;
  ST-002 baseline cases target due dates, not titles. Story-level
  qualification happens at ST-002 close.

## Expected handoff
- Diff, `node scripts/verify.mjs` result, AC to check mapping, updated
  behavior doc.
