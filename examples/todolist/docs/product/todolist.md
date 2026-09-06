# Todolist behavior contract

User-visible behavior of the todolist CLI and domain module.

## Getting usage

- `node src/cli.mjs help` prints the one-line usage text to stdout and exits
  0. The same text is printed when `--help` or `-h` appears anywhere on the
  command line (for example `node src/cli.mjs add --help`); in that case no
  command runs and the state file is neither read nor written, so the help
  flag works even when `.todolist.json` is corrupt.
- An unknown command or a command missing its arguments prints the same usage
  text to stderr and exits with code 2.

## Managing todos

- `node src/cli.mjs add <id> <title>` appends a new pending todo. The id must
  be unique and non-empty; the title is trimmed and must not be blank.
- `node src/cli.mjs add <id> <title> --due <YYYY-MM-DD>` additionally stores
  a due date. The value must be a real calendar date in `YYYY-MM-DD` form
  (for example `2026-12-01`); it is stored verbatim with no timezone
  conversion. `--due=<YYYY-MM-DD>` is accepted too, and the flag may appear
  anywhere after the id. Without `--due` the todo has no due date and the
  command behaves exactly as before.
- An invalid due date, such as `12/01/2026` (wrong format) or `2026-02-30`
  (not a calendar date), prints one stderr line beginning with `error:`,
  exits with code 2, and writes nothing to the state file.
- `node src/cli.mjs list` prints one pending todo per line as
  `<id>\t<title>`. A todo with a due date prints a third column:
  `<id>\t<title>\t<due>`. Undated todos keep the two-column line.
- The domain module exposes the same behavior as
  `createTodo(id, title, { due })`: a valid `due` is stored verbatim on the
  returned todo, an invalid one throws `TypeError`, and a todo created
  without `due` carries no `due` field at all (not `null`). State files
  written before due dates existed load unchanged; a missing `due` simply
  means the todo is undated.
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
