# Approval Gates

Pulse keeps a human approval model attached to artifacts and workflow state.

## Pre-gate admission

`pulse:workflow intake` is not a solution/design/execution approval. It is a new-work admission checkpoint that can run only when `pulse:workflow use` reports an empty session.

## Gate model overview

| Gate | When it happens | What gets approved | If not approved |
| --- | --- | --- | --- |
| Direction approval | after `brainstorm` when used | `work-brief.md` direction, scope, constraints | stay in `brainstorm` |
| Gate 1 | after `design` | `solution-design.md` final product/technical/solution decisions | stay in `design` or return to `explore` |
| Gate 2 | after `plan` | task breakdown/current-work shape derived from approved design | stay in `plan` or return to `design` if design gaps exist |
| Gate 3 | after `validate` | current execution slice is feasible and safe to start | do not execute |
| Gate 4 | after `review` | completed change is acceptable to merge or ship | fix findings before approval |

## Direction approval — Work brief

Purpose:

- confirm the work direction before discovery and solution design
- prevent discovery/design from optimizing the wrong outcome

Typical artifact:

- `work-brief.md` from `brainstorm`

Default next command after approval:

- `explore`

## Gate 1 — Solution design approval

Purpose:

- approve all product, technical, architecture, data, interface, migration, UX, and verification decisions before task planning
- prevent `plan` from inventing or changing solution approach

Typical artifact:

- `solution-design.md` from `design`

Default next command after approval:

- `plan`

## Gate 2 — Task plan approval

Purpose:

- confirm the task breakdown/current-work shape derived from approved solution design
- lock execution packaging and validation mapping without changing design

Typical artifacts:

- `PLAN.md` or current-work/task breakdown artifacts under the story directory

Default next command after approval:

- `validate`

## Gate 3 — Execution approval

Purpose:

- confirm that validation produced enough proof to begin implementation
- stop risky or underspecified work from silently slipping into execution

Validation should surface:

- blockers
- risk flags
- missing proof
- recommended execution mode

Default next command after approval:

- `swarm` when multi-agent execution is justified
- `execute` when single-agent execution is sufficient

## Gate 4 — Review approval

Purpose:

- separate implementation from evaluation
- make review findings actionable before merge or ship decisions

Rule:

- P1 findings block approval until fixed

Default next command after approval:

- `compound`

## State expectations

Runtime state should eventually express at least:

- current gate
- gate status
- active command
- recommended next command
- next action

A gate must never be marked approved without explicit user sign-off.

## Command posture around gates

- `intake` may ask for routing confirmation, but it does not approve direction, design, plan, execution, or review state.
- `brainstorm` may ask for direction sign-off, but it does not approve solution design.
- `explore` produces discovery evidence; it does not approve final solution design.
- `design` prepares Gate 1; it does not auto-approve it.
- `plan` prepares Gate 2 task breakdown; it must not change approved design.
- `validate` prepares Gate 3; it does not auto-start implementation.
- `review` prepares the merge or ship decision; it does not auto-merge.
