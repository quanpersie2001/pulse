# `/pulse brainstorm` Spec Reviewer Prompt

Use this when dispatching a spec self-review after brainstorming has produced a written design artifact.

**Purpose:** Verify the design is complete, internally consistent, and ready for `/pulse explore` to lock repo-grounded decisions without guesswork.

**Dispatch after:** the brainstorming output has been written and the team wants an independent consistency pass.

```text
Task tool (general-purpose):
  description: "Review brainstorm output"
  prompt: |
    You are reviewing a `/pulse brainstorm` output. Verify that the chosen direction is
    complete, internally consistent, and ready for the next workflow step.

    Artifact to review: [ARTIFACT_PATH]

    ## What to Check

    | Category | What to Look For |
    | --- | --- |
    | Completeness | TODOs, placeholders, unanswered design-critical questions |
    | Consistency | contradictions between goals, constraints, and the chosen direction |
    | Clarity | statements vague enough that planning could shape the wrong thing |
    | Scope | hidden second systems or side quests mixed into the same artifact |
    | YAGNI | unrequested capability growth or over-designed machinery |

    ## Calibration

    Only flag issues that would materially distort planning or execution.
    Ignore mere style preferences.

    ## Output Format

    **Status:** Approved | Issues Found

    **Issues (if any):**
    - [Section]: [specific issue] — [why it matters]

    **Recommendations (advisory only):**
    - [suggestion]
```
