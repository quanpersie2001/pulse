# ST-002 Approach — optional due dates

## Solution seam

Due dates enter through `createTodo` and flow through the existing pure
domain functions untouched; only `add` (CLI) parses the flag and `list`
(CLI) renders the extra column. The state file stays a plain JSON array;
`due` is an optional string field on a todo, absent when not set.

## Implementation decisions

- `createTodo(id, title, options?)` accepts an optional `{ due }` options
  object. When present it must be a `YYYY-MM-DD` string naming a real
  calendar date; otherwise it throws `TypeError`. When absent the created
  todo has no `due` field at all (not `due: null`), so pre-due consumers
  see no change.
- Validation lives in the domain module (pure, shared) not in the CLI:
  the CLI maps the `TypeError` to a stderr error line and exit code 2.
- `list` prints `<id>\t<title>\t<due>` for dated todos and keeps the
  existing two-column line for undated ones.
- Pre-due state files need no migration: a missing `due` field is simply
  an undated todo. The schema-evolution strategy question is TK-006's
  scope; this Story only relies on tolerant loading, which the current
  `loadTodos` already provides.

## Testing decisions

- Domain tests in `test/todolist.test.mjs` cover valid/invalid `due`,
  field absence on default creation, and input-list immutability.
- QA baseline cases QA-003 (roundtrip through the pure surface) and
  QA-004 (invalid format rejected) are the behavioral owner's proof.

## Out of scope

Recurring due dates, timezone-aware deadlines, sorting, editing an
existing todo's due, and any new command.
