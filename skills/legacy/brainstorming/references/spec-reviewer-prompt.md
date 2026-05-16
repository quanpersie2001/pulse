# Spec Document Reviewer Prompt

Use this when dispatching the spec self-review subagent in Phase 7 of brainstorming.

**Purpose:** Verify the spec is complete, consistent, and ready for exploring to lock implementation decisions.

**Dispatch after:** Spec is written to `history/<feature>/spec.md`

```
Task tool (general-purpose):
  description: "Review spec document"
  prompt: |
    You are a spec document reviewer. Verify this spec is complete and ready for
    pulse:exploring to lock implementation decisions.

    Spec to review: [SPEC_FILE_PATH]

    ## What to Check

    | Category     | What to Look For |
    |--------------|-----------------|
    | Completeness | TODOs, placeholders, "TBD", incomplete sections |
    | Consistency  | Internal contradictions, conflicting requirements |
    | Clarity      | Requirements ambiguous enough to cause building the wrong thing |
    | Scope        | Focused enough for a single feature cycle — not covering multiple independent subsystems |
    | YAGNI        | Unrequested features, over-engineering |

    ## Calibration

    Only flag issues that would cause real problems during implementation planning.
    A missing section, a contradiction, or a requirement so ambiguous it could be
    interpreted two different ways — those are issues. Minor wording improvements,
    stylistic preferences, or "sections less detailed than others" are not.

    Approve unless there are serious gaps that would lead to a flawed plan or force
    pulse:exploring to make wrong assumptions.

    ## Output Format

    **Status:** Approved | Issues Found

    **Issues (if any):**
    - [Section X]: [specific issue] — [why it matters for planning]

    **Recommendations (advisory, do not block approval):**
    - [suggestions for improvement]
```

**Reviewer returns:** Status, Issues (if any), Recommendations
