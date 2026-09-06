# TK-005 Todos can carry a due date

## Objective
Implement the ST-002 seam: `createTodo` accepts an optional `{ due }` with a
validated `YYYY-MM-DD` calendar date, `add --due` passes it through, and
`list` surfaces it as a third column for dated todos.

## Current behavior
Todos have no notion of due dates; `createTodo(id, title)` builds
`{ id, title, done: false }` and `list` prints two-column lines.

## Target behavior
- `createTodo(id, title, options?)` with `{ due: "YYYY-MM-DD" }` returns the
  todo with `due` set to exactly that string; absent options produce a todo
  with no `due` field (not `due: null`).
- A `due` that is not a `YYYY-MM-DD` string or not a real calendar date
  (e.g. `2026-02-30`) throws `TypeError`.
- `node src/cli.mjs add <id> <title> --due <date>` stores the date;
  `add` without `--due` is unchanged.
- `list` prints `<id>\t<title>\t<due>` for dated todos; undated todos keep
  the existing two-column line.
- The CLI maps a domain `TypeError` from a bad `--due` to one stderr line
  starting with `error:` and exit code 2, writing nothing.
- Pre-due state files load unchanged (missing `due` is undated).

## Code anchors
- src/todolist.mjs
- src/cli.mjs
- test/todolist.test.mjs
- scripts/qa-run.mjs
- docs/product/todolist.md

## Required changes
- Extend `createTodo` with options-validated `due`; keep every existing
  two-argument call site working.
- Parse `--due` in the CLI `add` case; wire the error mapping.
- Extend `list` rendering for dated todos.
- Add domain tests: valid date, nonexistent calendar date, wrong format,
  field absence by default, input immutability.
- Add QA runner handlers for baseline cases QA-003 and QA-004 in
  `scripts/qa-run.mjs` (the baseline itself is owned by ST-002 and must
  not change).
- Update `docs/product/todolist.md`: `add --due`, `list` third column,
  invalid-date behavior.

## Invariants
- Domain functions never mutate their inputs.
- Undated todos carry no `due` field; existing output for them is
  byte-identical.
- The `YYYY-MM-DD` string is stored verbatim; no timezone conversion.
- No new dependencies.

## Implementation freedom
guided: the seam, validation locus, output columns, and error contract are
fixed by the Story approach; internal parse structure is the worker's.

## Scope
- Domain options parameter, CLI flag, list rendering, tests, QA runner
  handlers, behavior doc.

## Non-scope
- Sorting by due date, editing an existing todo's due, `completed` output
  format, state-file versioning (owned by the TK-006 decision).

## Acceptance
- AC-1: `add --due 2026-12-01` stores `due` exactly and it survives a
  save/load roundtrip; `list` shows it as the third column.
- AC-2: `add` without `--due` and existing undated state files behave
  byte-identically to today.
- AC-3: `--due 12/01/2026` and `--due 2026-02-30` each print one stderr
  line starting with `error:`, exit 2, and write nothing.

## Verify
- node scripts/verify.mjs

## Open questions
- (resolved) Validation lives in the domain module so CLI and future callers share it — see works/ST-002/approach.md.
- (delegated) CLI argv parsing structure for the `--due` flag.

## Documentation impact
- Posture: required
- Documents: DOC-TODOLIST-BEHAVIOR
- Required update: document `add --due`, the `list` third column, and
  invalid-date behavior.

## QA impact
- Owner: ST-002
- Posture: required
- Cases: QA-003, QA-004
- Reason: new public behavior on creation and listing; baseline cases pin
  the pure-surface contract.

## Expected handoff
- Diff, `plan.md` (R2), `node scripts/verify.mjs` result, AC to check
  mapping, QA handler note, updated behavior doc.
