# Handoff and Resume

Handoffs preserve execution continuity across pauses, context limits, and ownership transitions.

## Canonical runtime paths

- `.pulse/runtime/state.json`
- `.pulse/runtime/STATE.md`
- `.pulse/runtime/handoffs/manifest.json`
- `.pulse/runtime/reservations.json`

## Principles

- handoffs are owner-scoped
- manifest is authoritative for active handoff selection
- resume must verify previous state is still current before acting

## Required handoff payload

A handoff should capture:

- active command and current work slice/item
- completed work and remaining work
- blockers/open questions
- reservation/conflict state when relevant
- read-first artifacts for fast restore
- recommended next action/command

## When to write handoffs

Write or refresh handoffs when:

- context budget approaches limit
- ownership changes
- execution pauses mid-slice
- blockers require asynchronous follow-up
- swarm worker or coordinator yields control

## Resume flow

1. read runtime state (`state.json`, `STATE.md`)
2. inspect handoff manifest
3. select owner handoff explicitly
4. read referenced artifacts
5. validate state freshness
6. continue same command or reroute to repair command

## Ownership transfer

Normal path: same owner resumes own handoff.

Cross-owner transfer requires explicit coordinator decision and manifest update recording:

- previous owner
- new owner
- transfer reason
- approval timestamp/identity

## Router implications

- `use` surfaces readiness, session restoration, and resumability
- `explore`/`plan`/`validate` may emit decision-state handoffs
- `swarm`/`execute` commonly emit operational handoffs
- `review`/`compound` may emit follow-up handoffs