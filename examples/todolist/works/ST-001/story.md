# ST-001 Todos can be completed reliably

## Outcome

A user of the todolist app can mark a todo as done and see a stable result:
completion of a known id reports `Completed`, an unknown id reports
`NotFound`, and neither path corrupts the caller's list. The behavior is
available through the domain module (`completeTodo`) and the CLI
(`node src/cli.mjs done <id>`), and the behavior contract doc describes it.

## Success signals

- QA-001 and QA-002 from `qa.md` pass on the integrated candidate.
- The CLI `done` command round-trips through the state file.

## Scope boundary

Completion behavior only: outcome names, list immutability, CLI wiring,
behavior doc. Persistence format and other commands are out of scope.
