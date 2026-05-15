# `/pulse swarm`

Maps from legacy `swarming`.

## Intent

Use this command when validated work should be executed by multiple agents with explicit coordination.

## Inputs expected

Bring:

- a validated work slice
- decomposition boundaries
- ownership expectations
- verification boundaries
- any coordination constraints for shared files or shared branch work

Useful shared references:

- `../../references/shared/swarm-execution-rules.md`
- `../../references/shared/handoff-and-resume.md`
- `../../references/shared/approval-gates.md`

## Primary outputs/artifacts

Typical outputs are:

- worker plan
- ownership boundaries
- handoff expectations
- review boundaries and escalation rules

## Interaction model

`swarm` is orchestration work.
It coordinates execution mode, boundaries, and handoffs. It should not present itself as a separate product surface outside the `/pulse` pipeline.

## Approval expectations

Use this only after Gate 3 approval.
If the swarm plan changes scope, ownership, or architecture in a meaningful way, get fresh confirmation before launching workers.

## Next command recommendations

- `execute` for the worker slices once orchestration is set
- `review` after workers complete their execution slices

## Failure / escalation behavior

- if the work cannot be decomposed safely, fall back to `execute`
- if coordination costs exceed the gain, say so and collapse the plan
- if the decomposition itself is broken, escalate to `rescue` or back to `plan`
