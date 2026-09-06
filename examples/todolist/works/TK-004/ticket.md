# TK-004 Fail safely when the state file is corrupt

## Objective
When `.todolist.json` is not valid JSON, every command must fail with one
clear error line instead of an unhandled stack trace, and must never
overwrite the corrupt file.

## Current behavior
`loadTodos` JSON-parses the state file; a corrupt file raises an unhandled
`SyntaxError` with a stack trace from whatever command ran. Worse, a
follow-up command that saves (e.g. `add`) would happily overwrite the
corrupt file with a freshly built list, silently destroying user data.

## Target behavior
- Any command run against a state file that fails to parse prints a single
  error line to stderr beginning with `error:` (must include the phrase
  `state file`), and exits with code 1.
- No command writes the state file when loading failed: the corrupt bytes
  reach the next command untouched.
- A valid state file behaves exactly as before (no regression in output or
  exit codes).

## Code anchors
- src/cli.mjs
- test/todolist.test.mjs
- docs/product/todolist.md

## Required changes
- Wrap state-file loading so parse failures become the controlled error
  path described above.
- Add tests covering the corrupt-file path: error output, exit code, and
  file-not-overwritten, using a temporary directory.
- Document the corrupt-file behavior in `docs/product/todolist.md`.

## Invariants
- The corrupt state file is never modified by any command.
- Recovery from corruption stays a manual user action (fix or delete the
  file); no command auto-deletes or auto-repairs.
- No new dependencies.

## Implementation freedom
guided: the worker chooses the internal structure (guard function, early
return, or exception wrapper); the stderr prefix `error:`, the phrase
`state file`, and exit code 1 are fixed.

## Scope
- Load-path error handling in the CLI, tests, behavior doc.

## Non-scope
- Backups, crash recovery, state file schema validation beyond JSON
  parsing, and any interactive prompt.

## Acceptance
- AC-1: With a syntactically invalid `.todolist.json`, `list` prints one
  stderr line starting with `error:` containing `state file` and exits 1.
- AC-2: After that failed run, the state file's bytes are unchanged (no
  command rewrote it).
- AC-3: With a valid state file, `list`, `add`, `done`, `completed`,
  `remove` behave exactly as before.

## Verify
- node scripts/verify.mjs

## Open questions
- (resolved) Exit code for corrupt state is 1 (runtime failure), not 2 (usage).
- (delegated) Exact wording after the `error:` prefix, as long as it contains `state file`.

## Documentation impact
- Posture: required
- Documents: DOC-TODOLIST-BEHAVIOR
- Required update: document corrupt-file behavior in "Managing todos".

## QA impact
- Posture: none
- Rationale: filesystem-bound error path exercised by focused node --test
  cases with temporary directories inside verify; ST-001 cases do not
  apply.

## Expected handoff
- Diff, `node scripts/verify.mjs` result, AC to check mapping, updated
  behavior doc.
