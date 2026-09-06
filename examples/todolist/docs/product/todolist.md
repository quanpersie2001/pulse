# Todolist behavior contract

User-visible behavior of the todolist CLI and domain module.

## Managing todos

- `node src/cli.mjs add <id> <title>` appends a new pending todo. The id must
  be unique and non-empty; the title is trimmed and must not be blank.
- `node src/cli.mjs list` prints one pending todo per line as
  `<id>\t<title>`.
- `node src/cli.mjs count` prints the number of pending todos as a plain
  integer on one line. An empty list prints `0`; done todos are not counted.
- `node src/cli.mjs remove <id>` drops the todo with that id.
- `node src/cli.mjs rename <id> <new title>` changes only the matched todo's
  title, trimming the new title while preserving its id, done state, position,
  and any other fields. It prints the stable outcome name `Renamed` and exits
  0; for an unknown id it prints `NotFound`, exits 1, and leaves the state file
  untouched. A blank title is rejected with `TypeError`.
- The domain module exposes the same behavior as
  `renameTodo(todos, id, title)`, which returns `{ outcome, todos }` and never
  mutates its input. `RenameOutcome` exports the stable names `Renamed` and
  `NotFound`.
- If `.todolist.json` is not valid JSON, every command prints one error line
  beginning with `error:` and containing `state file`, then exits with code 1.
  The corrupt file is never changed automatically; recovery requires manually
  fixing or deleting it.

## Listing completed todos

- `node src/cli.mjs completed` prints one done todo per line as
  `<id>\t<title>`, in insertion order. Pending todos are not shown; a list
  with no done todos prints nothing. Exit code is 0.
- The domain module exposes the same filter as `completedTodos(todos)`,
  which returns a new list of the done todos in insertion order and never
  mutates its input.

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
