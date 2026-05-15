# `/pulse systematic-debug`

Maps from legacy `systematic-debug-fix`.

## Intent

Use this command for disciplined bug investigation when the team needs evidence before choosing a fix.

## Inputs expected

Bring:

- the failing behavior and how to reproduce it
- error output, logs, or traces when available
- the affected scope or boundary
- any prior hypotheses or failed attempts

Useful shared references:

- `../../references/shared/verification-contract.md`
- `../../references/shared/handoff-and-resume.md`

## Primary outputs/artifacts

Typical outputs are:

- narrowed hypothesis set
- evidence summary
- likely root-cause direction
- fix or experiment recommendation

## Interaction model

`systematic-debug` is investigation-first work.
It may add targeted instrumentation or run controlled experiments, but it should not guess at a fix without evidence.

## Approval expectations

Ask for sign-off before turning a broad or risky debug conclusion into a large implementation move.

## Next command recommendations

- `execute` when the fix direction is now bounded and approved
- `review` when a small verified fix is already complete
- `rescue` when the failure exposes a deeper shape or architecture problem

## Failure / escalation behavior

- if reproduction is missing, say that before pretending the bug is understood
- if hypotheses stay too broad, narrow the search space instead of leaping to fixes
- if the investigation reveals a structural dead end, escalate to `rescue`
