# TK-003 Show the pending count with a count command

## Objective
Give users a one-line answer to "how much is left": `node src/cli.mjs count`
prints the number of pending todos as a single integer line.

## Current behavior
There is no `count` command; users pipe `list` through `wc -l` to approximate
it. Unknown command names print usage on stderr with exit code 2.

## Target behavior
- `node src/cli.mjs count` prints exactly one line: the number of pending
  todos, as a plain integer, then exits 0.
- An empty state prints `0`.
- Done todos are not counted; the command reads the same pending set as
  `list`.

## Code anchors
- src/cli.mjs

## Required changes
- Add a `count` case to the CLI switch computing `pendingTodos(todos).length`.
- Document the command in `docs/product/todolist.md` under "Managing todos".
- Add a focused CLI-level test in `test/todolist.test.mjs` or extend verify
  coverage per existing conventions.

## Invariants
- No changes to the domain module's public surface.
- No new dependencies.

## Implementation freedom
guided: the CLI switch layout is the worker's choice; output format
(plain integer, single line) and exit codes are fixed.

## Scope
- CLI command, behavior doc, focused test.

## Non-scope
- Counts of done todos, per-tag or per-date counts, any output format
  beyond the single integer line.

## Acceptance
- AC-1: `count` prints the number of pending todos as one integer line and
  exits 0.
- AC-2: With an empty state file, `count` prints `0` and exits 0.
- AC-3: Done todos are excluded from the count; completing a todo lowers
  the count by one.

## Verify
- node scripts/verify.mjs

## Open questions
- (delegated) One trailing newline, matching every other command's output.

## Documentation impact
- Posture: required
- Documents: DOC-TODOLIST-BEHAVIOR
- Required update: add `count` to the "Managing todos" section.

## QA impact
- Posture: none
- Rationale: read-only projection of the already-covered pending filter;
  focused verify tests exercise the command end to end.

## Expected handoff
- Diff, `node scripts/verify.mjs` result, AC to check mapping, updated
  behavior doc.
