# `pulse:workflow plan` Planning Reference

Use when `pulse:workflow plan` needs task/current-work quality rules.

## Quality rules

- Plan from approved `solution-design.md`; do not change solution decisions.
- Every task must trace to one or more design decision IDs or planning constraints.
- Tasks are worker-sized units for validated current work, not speculative future backlog.
- Use phases only for observable execution sequencing.
- Keep current work bounded; prepare one current slice for validation.
- MEDIUM/HIGH unknowns must be represented as validation evidence or routed back to design/explore if they change solution decisions.

Trace:

```text
solution-design.md -> PLAN.md -> current work -> work item?
```

## Planning modes

| Mode | Use when | Planning shape |
|---|---|---|
| `spike` | one approved-design assumption needs proof before execution | proof task/current work |
| `small_change` | <=3 files, LOW risk, approved design is simple | compact task plan |
| `standard_feature` | ordered user/system capability | phased task plan |
| `high_risk_feature` | hard-to-reverse, external/security/data, broad blast | risk-first task plan + validation emphasis |

Above `small_change`, record why smaller modes are insufficient.

## PLAN.md artifact

`PLAN.md` is the canonical task-planning artifact in each story directory.

It should capture:

- approved design source: `solution-design.md`
- planning mode and rationale
- task breakdown
- task dependencies and sequencing
- current-work slice
- validation mapping
- execution mode recommendation
- explicit design decision references

It must not capture new solution design, rejected design alternatives, schema/API/UX decisions, or architecture decisions not already approved in `solution-design.md`.

## Compact task plan template

```markdown
# Plan: <Work>

**Mode:** spike | small_change | standard_feature | high_risk_feature
**Source design:** `solution-design.md`
**Approval status:** PENDING | APPROVED

## Design Decisions Referenced

- D1 — <why relevant>
- D2 — <why relevant>

## Task Breakdown

| Task | Outcome | Depends On | Design IDs | Evidence |
| --- | --- | --- | --- | --- |
| T1 | ... | ... | D1 | ... |

## Current Work

- Entry state:
- Exit state:
- In scope:
- Out of scope:
- Validation evidence:

## Sequencing / Parallelization

- ...

## Risks for Validate

- ...

## Gate 2 Approval

Reply with explicit approval before validation begins.
```

## Current-work prep

Prepare only the approved current slice:

```markdown
# Current Work: <Slice>

Source plan: `PLAN.md`
Source design: `solution-design.md`
Entry state: <observable truth>
Exit state: <testable truth>
In scope: <bounded work>
Out of scope: <not solved>
Dependencies: <task/design dependencies>
Verification: <evidence required>
Work-item mapping: <created after validation accepts readiness, when applicable>
```

## Work item creation reminder

Planning may prepare item definitions only when workflow posture allows it. Do not create speculative future-slice work items.
