# Handoff and Resume

Pulse uses handoffs to survive context limits, ownership changes, and paused execution.

## Canonical runtime posture

The target runtime plane for handoff and resume is:

```text
.pulse/runtime/
```

Important locations:

- `.pulse/runtime/state.json`
- `.pulse/runtime/STATE.md`
- `.pulse/runtime/handoffs/manifest.json`
- `.pulse/runtime/checkpoints/`
- `.pulse/runtime/reservations.json`

## Handoff principles

- handoffs are owner-scoped
- the manifest is the authoritative index of active handoffs
- checkpoints are advisory snapshots, not the source of truth
- resume should start from the selected owner handoff plus current runtime state

## A good handoff should capture

- current command
- active work item or slice
- what was completed
- what remains
- blockers or open questions
- files or reservations in play
- what to read first on resume
- the recommended next command or action

## Resume flow

1. inspect the current runtime state
2. inspect the handoff manifest
3. choose the relevant owner handoff
4. read the referenced artifact set
5. confirm whether the previous state is still current
6. continue in the same command or route to a repair command

## When to write a handoff

Write or refresh a handoff when:

- context budget is getting tight
- ownership is changing
- execution pauses mid-slice
- a blocker needs another agent or a later session
- a swarm worker must yield safely

## Relationship to the router

- `onboard` can surface whether the runtime looks resumable
- `explore`, `plan`, and `validate` may leave decision-state handoffs
- `swarm` and `execute` commonly leave operational handoffs
- `review` and `compound` may leave follow-up handoffs

## Phase 1 note

Phase 1 creates the router contract for handoff and resume.
Later phases relocate the underlying runtime artifacts fully under `.pulse/runtime/`.
