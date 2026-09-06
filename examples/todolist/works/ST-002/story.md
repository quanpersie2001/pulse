# ST-002 Todos can carry due dates

## Outcome

A user can attach an optional due date to a todo when creating it, see that
date when listing, and get a clear rejection when the date is not a valid
ISO `YYYY-MM-DD` calendar date. Existing undated todos and every existing
command keep behaving exactly as before. The behavior is available through
the domain module (optional `due` on created todos) and the CLI
(`add --due`), and the behavior contract doc describes it.

## Success signals

- QA-003 and QA-004 from `qa.md` pass on the integrated candidate.
- An `add --due` round-trips through the state file and shows up in `list`.
- Pre-due state files (no `due` field) still load without error.

## Scope boundary

Optional due dates on creation, their display in `list`, format validation,
and the behavior doc. Editing a due after creation, sorting by due,
completion-date tracking, and reminders are out of scope.
