# ST-002 QA baseline — Optional due dates

Due dates promise that a valid `YYYY-MM-DD` string is stored and surfaced
unchanged, that invalid input is rejected before any state is touched, and
that undated todos are indistinguishable from pre-due-era todos. These
cases are the behavioral owner's definition of "reliable due dates" for
every Ticket that touches creation or listing.

```pulse-qa
{
  "schema_version": 1,
  "story_id": "ST-002",
  "revision": 1,
  "scope": "Optional due dates are stored, surfaced, and validated without corrupting undated todos",
  "risks": ["RISK-BAD-DATE", "RISK-MUTATION"],
  "cases": [
    {
      "id": "QA-003",
      "revision": 1,
      "intent": "A todo created with a valid due date carries that date through the pure surface while the input list is never mutated",
      "priority": "high",
      "risk_refs": ["RISK-MUTATION"],
      "steps": [
        "Call createTodo(\"qa-3\", \"sample\", { due: \"2026-12-01\" })",
        "Add it to a list with one undated todo via addTodo",
        "Inspect the dated todo and the input list"
      ],
      "expected": [
        "the created todo has due exactly \"2026-12-01\" and done false",
        "the undated todo in the same list has no due field",
        "the input list object is unchanged"
      ],
      "surface": "api",
      "applicability": "required"
    },
    {
      "id": "QA-004",
      "revision": 1,
      "intent": "An invalid due date is rejected with TypeError and nothing is created or mutated",
      "priority": "high",
      "risk_refs": ["RISK-BAD-DATE"],
      "steps": [
        "Call createTodo(\"qa-4\", \"sample\", { due: \"12/01/2026\" })",
        "Call createTodo(\"qa-4b\", \"sample\", { due: \"2026-02-30\" })"
      ],
      "expected": [
        "each call throws TypeError",
        "no todo value is returned and no list is involved"
      ],
      "surface": "api",
      "applicability": "required"
    }
  ],
  "exit_criteria": [
    "Every required case passes on the candidate source"
  ]
}
```
