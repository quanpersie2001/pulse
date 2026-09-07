# ST-002 QA baseline — Optional due dates

Due dates promise that a valid `YYYY-MM-DD` string is stored and surfaced
unchanged, that invalid input is rejected before any state is touched, and
that undated todos are indistinguishable from pre-due-era todos. These
cases are the behavioral owner's definition of "reliable due dates" for
every Ticket that touches creation or listing.

## Scope
Optional due dates are stored, surfaced, and validated without corrupting undated todos.

## Posture
automated

## Risks
- RISK-BAD-DATE: an impossible date reaches the state file and corrupts stored data.
- RISK-MUTATION: creating a dated todo mutates the caller's list.

## Exit criteria
- Every required case passes on the candidate source.

## Cases

### QA-003 A valid due date survives creation and listing
- Intent: A todo created with a valid due date carries that date through the pure surface while the input list is never mutated.
- Surface: api
- Priority: high
- Risks: RISK-MUTATION
- Preconditions:
  - A list holding one undated todo.
- Steps:
  1. Call createTodo("qa-3", "sample", { due: "2026-12-01" }).
  2. Add it to the list with addTodo.
  3. Inspect the dated todo, its undated sibling, and the input list.
- Expected:
  - the created todo has due exactly 2026-12-01 and done false.
  - the undated todo in the same list has no due field.
  - the input list object is unchanged.
- Evidence:
  - stdout of the case check.

```pulse-check
run: node scripts/qa-case.mjs QA-003
assert:
  - exit_code: 0
  - stdout_line: QA-003 ok
  - stdout_contains: the input list was not mutated
```

### QA-004 An impossible due date is rejected before anything is created
- Intent: An invalid due date is rejected with TypeError and nothing is created or mutated.
- Surface: api
- Priority: high
- Risks: RISK-BAD-DATE
- Preconditions:
  - No state is required; the surface is pure.
- Steps:
  1. Call createTodo("qa-4", "sample", { due: "12/01/2026" }).
  2. Call createTodo("qa-4b", "sample", { due: "2026-02-30" }).
- Expected:
  - each call throws TypeError.
  - no todo value is returned and no list is involved.
- Evidence:
  - stdout of the case check.

```pulse-check
run: node scripts/qa-case.mjs QA-004
assert:
  - exit_code: 0
  - stdout_line: QA-004 ok
  - stdout_contains: without returning a todo
```
