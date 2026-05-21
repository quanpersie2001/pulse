# Approval Gates

Pulse keeps a human approval model attached to artifacts and workflow state.

## Gate model overview

| Gate | When it happens | What gets approved | If not approved |
| --- | --- | --- | --- |
| Gate 1 | after `explore` | context artifact and locked decisions | stay in `explore` |
| Gate 2 | after `plan` | selected execution shape artifact | stay in `plan` or revisit `brainstorm` |
| Gate 3 | after `validate` | current execution slice is feasible and safe to start | do not execute |
| Gate 4 | after `review` | completed change is acceptable to merge or ship | fix findings before approval |

## Gate 1 — Context approval

Purpose:

- confirm the current problem framing
- confirm constraints and decisions that later commands must honor
- prevent planning from guessing at product or architecture intent

Typical artifact:

- a context synthesis produced by `explore`

Default next command after approval:

- `plan`

## Gate 2 — Shape approval

Purpose:

- confirm the proposed execution shape before validation or implementation
- lock the work breakdown and artifact model the rest of the cycle will follow

Typical artifacts:

- work-shape
- phase-plan
- epic-map

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

- `brainstorm` may ask for directional sign-off, but it does not replace Gate 1 or Gate 2.
- `plan` prepares the shape; it does not auto-approve it.
- `validate` prepares the execution case; it does not auto-start implementation.
- `review` prepares the merge or ship decision; it does not auto-merge.
