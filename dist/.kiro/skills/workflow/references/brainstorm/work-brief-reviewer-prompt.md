# `pulse:workflow brainstorm` Work Brief Reviewer Prompt

Use this after brainstorming has produced a written `work-brief.md`.

**Purpose:** Verify the brief is complete, intake-aligned, directionally clear, and not leaking into implementation design.

```text
Task tool (general-purpose):
  description: "Review brainstorm work brief"
  prompt: |
    Review this `pulse:workflow brainstorm` output.

    Work brief: [BRIEF_PATH]
    Intake: [INTAKE_PATH]

    Check only issues that would materially distort downstream work.
    Ignore style preferences.

    ## Check

    | Category | What to Look For |
    | --- | --- |
    | Intake Alignment | stays inside confirmed boundary, lane, risk flags, surfaces, obligations, and routing posture |
    | Completeness | no TODOs/placeholders; required sections are answered or marked N/A |
    | Direction Clarity | selected direction, rationale, outcome, scope, and non-goals are clear |
    | Alternatives | viable options and trade-offs are recorded |
    | Consistency | no contradictions between outcome, constraints, behavior, and direction |
    | Scope | no hidden second systems or side quests |
    | YAGNI | no unrequested capability growth or over-designed direction |
    | Open Assumptions | unresolved repo-fit or evidence questions are named |
    | Technical Boundary | technical content is directional only, not implementation design |
    | Implementation Leakage | no concrete schema, ERD, module/file ownership, interfaces, migration plan, detailed test plan, validation commands, or execution sequence |

    ## Calibration

    Acceptable: "prefer direct handlers over MediatR" when framed as direction.
    Not acceptable: concrete handler interfaces, folder layout, DI wiring, migration sequence, or test plan.

    Acceptable: data ownership/isolation intent.
    Not acceptable: concrete tables, fields, indexes, ERDs, migration/backfill, or final partitioning strategy.

    ## Output Format

    **Status:** Approved | Issues Found

    **Issues (if any):**
    - [Section]: [specific issue] — [why it matters]

    **Recommendations (advisory only):**
    - [suggestion]
```
