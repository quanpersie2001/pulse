# `/pulse rescue`

Maps from legacy `architecture-rescue`.

## Intent

Use this command when implementation is stuck because the current shape is wrong, the architecture drifted, or the team is pushing deeper into a dead end.

## Inputs expected

Bring:

- the failing work slice or stuck execution path
- current architecture constraints
- observed symptoms of drift or dead-end work
- the last plan, validation, or review artifacts that led here

Useful shared references:

- `../../references/shared/workflow-contract.md`
- `../../references/shared/handoff-and-resume.md`
- `../../references/shared/approval-gates.md`

## Primary outputs/artifacts

Typical outputs are:

- recovery framing
- reset or re-scope options
- recommended safer path
- explicit statement of what should stop immediately

## Interaction model

`rescue` is intervention design.
It diagnoses why the current path is wrong and proposes a better path without pretending the existing execution can simply be pushed harder.

## Approval expectations

If the recovery changes architecture, scope, or ownership meaningfully, get explicit sign-off before switching paths.

## Next command recommendations

- `plan` when the work needs a different shape
- `validate` when the new path is defined but still needs proof
- `execute` only when the fix is already obvious and newly bounded

## Failure / escalation behavior

- if the issue is actually a live bug investigation, route to `systematic-debug`
- if there is not enough evidence to diagnose the drift, gather it before recommending a reset
- if the current work is unsafe, say so plainly and stop execution first
