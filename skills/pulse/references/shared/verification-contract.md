# Verification Contract

Verification is how Pulse proves a proposed or completed move is real, not assumed.

## Verification by command

| Command | Verification focus |
| --- | --- |
| `validate` | feasibility, risk, missing proof, execution readiness |
| `execute` | implementation evidence for the active work item |
| `review` | correctness, regressions, severity, merge readiness |
| work-item close | minimum mechanical evidence required to mark work complete |

## `validate`

`validate` should answer:

- can this shape be executed safely?
- what proof already exists?
- what proof is still missing?
- what blockers or risks remain?
- what execution mode is justified?

Expected output:

- execution-readiness call
- risk surface
- blockers
- concrete evidence or validation gaps
- next-command recommendation

If validation is weak or contradictory, route back to `plan` or `brainstorm`.

## `execute`

`execute` should leave behind evidence that the change actually works.

Examples of evidence:

- commands run
- tests added or updated
- observed outputs
- generated artifacts
- unresolved gaps called out explicitly

Execution without evidence is incomplete work.

## `review`

`review` is a distinct evaluation pass.

It should classify findings clearly and call out:

- correctness issues
- regression risk
- missing tests or weak evidence
- policy or contract violations
- whether P1 findings remain

P1 findings block approval.

## Work-item close posture

In the target runtime model, `TASK` and `BUG` close should require a valid verification artifact.

At minimum, that artifact should show:

- evidence summary
- commands run
- observed outputs
- attempts
- artifacts
- unresolved gaps

## Router implications

- `plan` may propose verification work, but it does not satisfy verification.
- `validate` proves the path into execution.
- `execute` produces first-party evidence.
- `review` decides whether that evidence is sufficient.
