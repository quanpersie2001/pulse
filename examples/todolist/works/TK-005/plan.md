# TK-005 plan — due dates (R2)

Written by `agent:runner:worker` after reading the checkout at
`b2fa674`. The seam, validation locus, output columns and error contract
come from `works/ST-002/approach.md`; this plan records the concrete
placement and the delegated choices.

## What changes, where

1. `src/todolist.mjs` — `createTodo(id, title, options = {})`.
   - `options.due === undefined` (no options, `{}`, or `{ due: undefined }`)
     returns `{ id, title, done: false }` with no `due` key, so every
     existing two-argument call site is byte-for-byte unchanged.
   - Otherwise `due` must pass `isCalendarDate`: a `^\d{4}-\d{2}-\d{2}$`
     match plus a month range and days-in-month check with an explicit
     Gregorian leap rule. No `Date` parsing, so no timezone interpretation;
     the string is stored verbatim as `{ ...todo, due }`.
   - Failures throw `TypeError("due must be a YYYY-MM-DD calendar date")`.
     A non-object `options` also throws `TypeError`.
   - No other domain function is touched: `addTodo`, `completeTodo`,
     `renameTodo` spread the todo, so `due` flows through unchanged.

2. `src/cli.mjs` — `add` and `list` only.
   - `parseAddArgs(rest)` (delegated parse structure): pulls `--due <v>` or
     `--due=<v>` from anywhere after the id, everything else is title. A
     dangling `--due` is a usage error (exit 2, nothing written). Without
     the flag the title words are exactly `rest`, matching pre-due behavior.
   - `createTodo` is called with `{ due }` only when the flag was given.
     A `TypeError` thrown for a dated add is mapped to one stderr line
     `error: <message>`, `exitCode = 2`, and the `saveTodos` call is never
     reached. Undated adds keep the pre-due path (no mapping), so their
     failure behavior is unchanged.
   - `list` prints `<id>\t<title>\t<due>` when `todo.due != null`, else the
     existing two-column line. `completed` output is untouched (non-scope).
   - The usage string gains `[--due <YYYY-MM-DD>]`. This is the one
     deliberate change visible to undated usage: it only affects the usage
     error line, not any todo output.

3. `test/todolist.test.mjs` — seven new tests tagged `TK-005 AC-n`:
   valid date + input/options immutability (QA-003 mirror), leap-day
   acceptance, absent-field default, an invalid-value table (wrong format,
   `2026-02-30`, `2023-02-29`, `2026-04-31`, month/day out of range,
   unpadded, datetime, empty, null, number, Date) (QA-004 mirror), CLI
   roundtrip + third column (AC-1), pre-due state file and undated add
   byte-identical (AC-2), invalid `--due` error contract with and without an
   existing state file plus dangling `--due` (AC-3).

4. `scripts/qa-run.mjs` — handlers for `QA-003` and `QA-004` following the
   existing `QA-001`/`QA-002` shape. `works/ST-002/qa.md` is not edited.

5. `docs/product/todolist.md` — "Managing todos" documents `add --due`,
   the invalid-date error contract, the `list` third column, and the domain
   `createTodo(id, title, { due })` contract including tolerant loading of
   pre-due state files.

## Decisions taken inside implementation freedom

- `{ due: undefined }` is treated as "not provided" rather than rejected,
  so callers can pass through an optional value without branching.
- Validation is pure arithmetic instead of `Date` round-tripping, to honor
  the "no timezone conversion" invariant literally.
- The error mapping in the CLI is scoped to dated adds so AC-2's
  byte-identical promise for undated adds holds even on failure paths.
- State-file versioning is untouched, per TK-006: a missing `due` is an
  undated todo and `loadTodos` already tolerates it.

## Verification

`node scripts/verify.mjs` (27 tests), targeted `node --test` runs, a scratch
directory CLI walk-through, and `node scripts/qa-run.mjs` against QA-003 and
QA-004. Results are in `validation.md`.
