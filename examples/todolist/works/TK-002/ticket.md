# TK-002 List completed todos in the CLI

## Objective
Add a `completed` command to the todolist CLI that lists done todos, so
users can review finished work the same way `list` shows pending work.

## Current behavior
`node src/cli.mjs list` shows only pending todos; there is no way to list
completed ones.

## Target behavior
- `node src/cli.mjs completed` prints one done todo per line as
  `<id>\t<title>`, in insertion order.
- The module gains `completedTodos(todos)` filtering done items.

## Code anchors
- src/todolist.mjs
- src/cli.mjs
- test/todolist.test.mjs
- docs/product/todolist.md

## Required changes
- Add `completedTodos` to `src/todolist.mjs`.
- Wire a `completed` command in `src/cli.mjs`.
- Add a unit test for the filter.
- Document the command in `docs/product/todolist.md`.

## Invariants
- Functions never mutate their `todos` argument.
- No new dependencies.

## Implementation freedom
guided: agent chooses internal structure; the command name and output
format are fixed.

## Scope
- One module function, one CLI command, one test, doc section.

## Non-scope
- Undo/uncomplete behavior; sorting; persistence format changes.

## Acceptance
- AC-1: `completed` lists only done todos in insertion order as
  `<id>\t<title>`.

## Verify
- node scripts/verify.mjs

## Open questions
- (delegated) Internal helper structure: worker chooses.

## Documentation impact
- Posture: required
- Documents: DOC-TODOLIST-BEHAVIOR
- Required update: add the completed listing to the behavior contract.

## QA impact
- Owner:
- Posture: none
- Cases:
- Reason: read-only listing; completion behavior itself is unchanged.

## Expected handoff
- Diff, `node scripts/verify.mjs` result, AC to check mapping, updated
  behavior doc.
