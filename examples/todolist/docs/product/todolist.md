# Todolist behavior contract

User-visible behavior of the todolist CLI and domain module.

## Managing todos

- `node src/cli.mjs add <id> <title>` appends a new pending todo. The id must
  be unique and non-empty; the title is trimmed and must not be blank.
- `node src/cli.mjs list` prints one pending todo per line as
  `<id>\t<title>`.
- `node src/cli.mjs remove <id>` drops the todo with that id.

## Completing todos

- `node src/cli.mjs done <id>` marks the todo with that id as done in the
  state file and prints the outcome name on its own line.
- Outcomes are stable public names, not exceptions:
  - `Completed`: a todo with that id existed; it is now done and no longer
    appears in `list`. Exit code is 0.
  - `NotFound`: no todo has that id; the state file is left untouched. Exit
    code is 1.
- Completing an already-done todo reports `Completed` and is a no-op.
- The domain module exposes the same behavior as
  `completeTodo(todos, id)`, which returns `{ outcome, todos }`. The `todos`
  in the result is a new list on `Completed` and the input list on
  `NotFound`; the caller's input list is never mutated. `CompleteOutcome`
  exports the outcome names as constants.
