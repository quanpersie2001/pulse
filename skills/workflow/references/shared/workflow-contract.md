# `pulse:workflow` Workflow Contract

This document describes the Pulse workflow pipeline in router language.

## Router contract

`pulse:workflow` is the only public workflow surface.

- `pulse:workflow` renders help.
- `pulse:workflow <command>` loads that command's contract.
- Unknown commands fall back to help.
- The router never dispatches silently to another workflow surface.

## Canonical command pipeline

```text
pulse:workflow use
  -> pulse:workflow intake         (only when use reports an empty session and new user input exists)
  -> pulse:workflow brainstorm     (optional when work direction is still vague)
  -> pulse:workflow explore
  -> pulse:workflow design
  -> pulse:workflow plan
  -> pulse:workflow validate
  -> pulse:workflow swarm | pulse:workflow execute
  -> pulse:workflow review
  -> pulse:workflow compound
```

When execution gets stuck or needs deeper tactical support, reroute to standalone utilities instead of adding router subcommands:

- `pulse:architecture-rescue`
- `pulse:systematic-debug-fix`
- `pulse:dev-note`
- `pulse:dev-note-distil`

## Command responsibilities

| Command | Primary responsibility |
| --- | --- |
| `use` | prepare the repo if needed, load the current session, and surface runtime posture |
| `intake` | admit new user input only when the session is empty, then classify type, lane, artifact obligations, and next command |
| `brainstorm` | shape vague intent into candidate directions and an approved `work-brief.md` |
| `explore` | gather repo/domain/external evidence and surface design decision questions in `discovery.md` |
| `design` | turn direction and discovery into approved final product/technical/solution decisions in `solution-design.md` |
| `plan` | decompose approved solution design into tasks/current-work contracts without changing design |
| `validate` | prove the planned work is executable and expose risk before work starts |
| `swarm` | orchestrate validated multi-agent execution |
| `execute` | implement a validated work item |
| `review` | evaluate completed changes and enforce the final quality gate |
| `compound` | capture reusable learning after the cycle |

## Gated progression

The router keeps the human-gate model attached to artifacts and runtime state.

`intake` is a pre-gate admission checkpoint for new work. It can run only after `use` reports an empty session.

1. after `brainstorm` when used, approve direction in `work-brief.md`
2. after `design`, approve final solution decisions in `solution-design.md`
3. after `plan`, approve task breakdown/current-work shape
4. after `validate`, explicitly approve execution
5. after `review`, resolve blocking findings before merge or ship approval

See `approval-gates.md` for gate details.

## Design immutability after approval

Once `solution-design.md` is approved:

- `plan` may only decompose and sequence work
- `validate` may only prove readiness or route back
- `execute` may only implement the approved plan

If any later phase discovers that product behavior, approach, architecture, schema, API, UX, migration, or verification strategy must change, it must stop and route back to `pulse:workflow design` or `pulse:workflow explore`.

## Router vs runtime boundary

Use the router to decide **what workflow move should happen next**.

Use `{{pulse_command}}` to inspect readiness and coordinate runtime reservations once the runtime exists.

Examples:

- `pulse:workflow design` decides the solution.
- `pulse:workflow plan` decomposes approved design into work.
- `{{pulse_command}} ready --repo-root <repo> --json` surfaces executable work.

The router is conversational and decision-oriented.
The runtime is mechanical and state-oriented.

## Failure posture

If a command cannot proceed because required context, approvals, or proof are missing:

- stop in the current command
- say what is missing
- recommend the earlier command that should repair it
- avoid hidden transitions or silent state mutation
