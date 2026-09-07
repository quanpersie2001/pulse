# ST-001 QA baseline — Completing todos

The completion feature promises that completing a todo is observable through
stable outcome names and never corrupts the caller's list. These cases are
the behavioral owner's definition of "reliable completion" for every Ticket
that touches completion.

## Scope
Completing todos returns stable outcomes and preserves list identity.

## Posture
automated

## Risks
- RISK-UNKNOWN-ID: completing an id nobody owns silently changes stored data.
- RISK-MUTATION: completion mutates the caller's list in place.

## Exit criteria
- Every required case passes on the candidate source.

## Cases

### QA-001 Completing an existing todo reports Completed
- Intent: Completing an existing todo reports Completed and marks it done without mutating the caller's list.
- Surface: api
- Priority: high
- Risks: RISK-MUTATION
- Preconditions:
  - A list holding one pending todo.
- Steps:
  1. Build a list with one pending todo qa-1.
  2. Call completeTodo(todos, "qa-1").
  3. Compare the returned list with the input list.
- Expected:
  - outcome is Completed.
  - the returned list marks qa-1 done.
  - the input list is unchanged.
- Evidence:
  - stdout of the case check.

```pulse-check
run: node scripts/qa-case.mjs QA-001
assert:
  - exit_code: 0
  - stdout_line: QA-001 ok
  - stdout_contains: without mutating the input
```

### QA-002 Completing an unknown id reports NotFound
- Intent: Completing an id that is not in the list reports NotFound and changes nothing.
- Surface: api
- Priority: high
- Risks: RISK-UNKNOWN-ID
- Preconditions:
  - A list holding one already-done todo.
- Steps:
  1. Call completeTodo(todos, "missing-id").
- Expected:
  - outcome is NotFound.
  - the returned list equals the input list.
- Evidence:
  - stdout of the case check.

```pulse-check
run: node scripts/qa-case.mjs QA-002
assert:
  - exit_code: 0
  - stdout_line: QA-002 ok
  - stdout_contains: left the list unchanged
```
