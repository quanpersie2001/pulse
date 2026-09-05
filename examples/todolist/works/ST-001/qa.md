# ST-001 QA baseline — Completing todos

The completion feature promises that completing a todo is observable through
stable outcome names and never corrupts the caller's list. These cases are
the behavioral owner's definition of "reliable completion" for every Ticket
that touches completion.

```pulse-qa
{
  "schema_version": 1,
  "story_id": "ST-001",
  "revision": 1,
  "scope": "Completing todos returns stable outcomes and preserves list identity",
  "risks": ["RISK-UNKNOWN-ID", "RISK-MUTATION"],
  "cases": [
    {
      "id": "QA-001",
      "revision": 1,
      "intent": "Completing an existing todo reports Completed and marks it done without mutating the input list",
      "priority": "high",
      "risk_refs": ["RISK-MUTATION"],
      "steps": [
        "Build a list with one pending todo t1",
        "Call completeTodo(todos, \"t1\")",
        "Compare the returned list and the input list"
      ],
      "expected": [
        "outcome is Completed",
        "the returned list marks t1 done",
        "the input list is unchanged"
      ],
      "surface": "api",
      "applicability": "required"
    },
    {
      "id": "QA-002",
      "revision": 1,
      "intent": "Completing an unknown id reports NotFound and changes nothing",
      "priority": "high",
      "risk_refs": ["RISK-UNKNOWN-ID"],
      "steps": [
        "Build a list with one already-done todo",
        "Call completeTodo(todos, \"missing-id\")"
      ],
      "expected": [
        "outcome is NotFound",
        "the returned list equals the input list"
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
