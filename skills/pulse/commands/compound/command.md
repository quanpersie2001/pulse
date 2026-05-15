# `/pulse compound`

Maps from legacy `compounding` and absorbs any remaining reusable value that used to sit near `dream`.

## Intent

Use this command after a workflow cycle to capture reusable learning, sharpen the harness, and improve future execution quality.

## Inputs expected

Bring:

- completed work and review outcomes
- verification evidence and unresolved gaps
- notes, patterns, or workflow friction worth preserving
- any repeated mistakes or surprising wins from the cycle

Useful shared references:

- `../../references/shared/verification-contract.md`
- `../../references/HARNESS.md`

## Primary outputs/artifacts

Typical outputs are:

- reusable learnings
- guidance for future cycles
- harness improvement ideas
- distilled follow-up recommendations

## Interaction model

`compound` is synthesis work.
It may write learnings or backlog follow-ups, but it should not re-open execution silently.

## Approval expectations

No fixed human gate is required, but ask before changing shared contracts or publishing durable guidance beyond the current cycle.

## Next command recommendations

- `note-distill` when raw notes still need consolidation
- `plan` when the learning immediately informs the next work slice

## Failure / escalation behavior

- if the work is not actually complete yet, route back to `review` or `execute`
- if the learning is too raw, capture it with `note` first instead of forcing premature synthesis
