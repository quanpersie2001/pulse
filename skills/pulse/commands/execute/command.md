# `/pulse execute`

Maps from legacy `executing`.

## Intent

Use this command to implement a validated work item and produce the evidence needed for later review.

## Inputs expected

Bring:

- the validated work slice
- the approved execution boundary
- relevant code context and constraints
- verification expectations
- any active handoff or reservation posture

Useful shared references:

- `../../references/shared/verification-contract.md`
- `../../references/shared/handoff-and-resume.md`
- `../../references/shared/swarm-execution-rules.md`

## Primary outputs/artifacts

Typical outputs are:

- implemented changes
- verification evidence
- execution notes or handoff updates
- a review-ready summary of what changed

## Interaction model

`execute` is the main mutating command.
It may edit code, run verification commands, and later coordinate with `pulse-work` once the runtime layer is relocated.
In Phase 1, it mainly defines that contract.

## Approval expectations

Only use this after Gate 3 approval.
If execution reveals a major shape change, stop and get fresh approval before widening scope.

## Next command recommendations

- `review` in the normal case
- `rescue` when the implementation path is structurally wrong
- `systematic-debug` when the blocker is a live failure that needs investigation first

## Failure / escalation behavior

- if the work item is underspecified, route back to `validate` or `plan`
- if the path is blocked by architectural drift, route to `rescue`
- if the work turns into bug investigation, route to `systematic-debug`
