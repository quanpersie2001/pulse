# Verification Contract

Verification proves readiness and completion with observable evidence.

## Cross-command rules

- No execution without Gate 3 approval.
- No work-item close without fresh, scoped evidence.
- No merge/ship recommendation while P1 findings are open.
- Validation, execution, and review are separate responsibilities.

## `validate` evidence contract

`validate` must produce:

- readiness call (`ready`, `ready-with-constraints`, `not-ready`)
- blocker list with concrete remediation path
- risk surface tied to current slice boundaries
- explicit missing-proof list
- execution mode recommendation (`swarm` or `single-worker`)

When assumptions are unresolved and high-impact, require targeted probes/spikes before readiness approval.

## `execute` evidence contract

Each completed item should include an evidence record containing:

- commands/tests run
- observed outputs
- artifacts produced
- unresolved gaps (explicitly `None.` when none remain)

Evidence must be from the current run, not copied from prior sessions.

## `review` evidence contract

`review` must evaluate:

- correctness against approved boundaries
- regression risk and missing coverage
- contract/policy violations
- severity-classified findings

P1 findings block Gate 4 approval until resolved.

## Minimum mechanical close standard (`TASK`/`BUG`)

Close should require a non-empty verification artifact with at least:

- `## Evidence Summary`
- `## Commands Run`
- `## Observed Outputs`
- `## Attempts`
- `## Artifacts`
- `## Unresolved Gaps`

`## Unresolved Gaps` must explicitly state remaining gaps or `None.`.

## Routing implications

- If validation proof is insufficient, route back to `plan`.
- If execution fails without clear cause, route to `systematic-debug`.
- If review finds structural mismatch, route to `rescue` or `plan` depending on blast radius.