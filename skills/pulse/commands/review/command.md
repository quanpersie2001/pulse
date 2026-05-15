# `/pulse review`

Maps from legacy `reviewing`.

## Intent

Use this command to evaluate completed work after execution and before merge or ship approval.

## Inputs expected

Bring:

- the implemented change set
- verification evidence from execution
- the approved plan or validation boundary
- any known areas of risk that deserve extra scrutiny

Useful shared references:

- `../../references/shared/approval-gates.md`
- `../../references/shared/verification-contract.md`

## Primary outputs/artifacts

Typical outputs are:

- review findings
- severity calls
- merge or ship recommendation
- explicit follow-up actions when findings block approval

## Interaction model

`review` is read and evaluation work.
It should stay separate from primary execution, even when the reviewer recommends changes.

## Approval expectations

This command prepares Gate 4.
P1 findings block approval until fixed.

## Next command recommendations

- `compound` when review passes and the cycle is closing cleanly
- `execute` when review findings require implementation fixes

## Failure / escalation behavior

- if evidence is missing, call that out explicitly instead of pretending the review is complete
- if the change crosses boundaries the plan did not authorize, send it back for repair before approval
- if the right move is broader recovery rather than a small fix, escalate to `rescue`
