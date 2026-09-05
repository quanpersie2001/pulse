# Todolist behavior contract

User-visible behavior of the todolist CLI and domain module.

## Managing todos

- `node src/cli.mjs add <id> <title>` appends a new pending todo. The id must
  be unique and non-empty; the title is trimmed and must not be blank.
- `node src/cli.mjs list` prints one pending todo per line as
  `<id>\t<title>`.
- `node src/cli.mjs remove <id>` drops the todo with that id.

## Completing todos

Not yet available. Completing a todo must use stable outcome names
(`Completed` for a known id, `NotFound` for an unknown id) so callers and
tests can match on outcomes instead of exceptions.
