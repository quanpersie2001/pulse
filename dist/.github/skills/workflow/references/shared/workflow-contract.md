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
  -> pulse:workflow brainstorm     (optional when feature shape is still vague)
  -> pulse:workflow explore
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
| `explore` | understand the codebase, current state, and implementation-relevant decision context |
| `brainstorm` | shape vague intent into candidate approaches and an approved design before exploration |
| `plan` | turn context into a concrete implementation shape |
| `validate` | prove the shape is executable and expose risk before work starts |
| `swarm` | orchestrate validated multi-agent execution |
| `execute` | implement a validated work item |
| `review` | evaluate completed changes and enforce the final quality gate |
| `compound` | capture reusable learning after the cycle |

## Gated progression

The router keeps the human-gate model attached to artifacts and runtime state.

`intake` is a pre-gate admission checkpoint for new work. It can run only after `use` reports an empty session, and it does not replace Gate 1.

1. after `explore`, approve the context artifact
2. after `plan`, approve the selected shape artifact
3. after `validate`, explicitly approve execution
4. after `review`, resolve blocking findings before merge or ship approval

See `approval-gates.md` for the gate details.

## Router vs runtime boundary

Use the router to decide **what workflow move should happen next**.

Use `node .github/skills/workflow/scripts/pulse.mjs` to inspect readiness and coordinate runtime reservations once the runtime exists.

Examples:

- `pulse:workflow plan` decides the shape of work.
- `node .github/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json` surfaces executable work.

The router is conversational and decision-oriented.
The runtime is mechanical and state-oriented.

## Failure posture

If a command cannot proceed because required context, approvals, or proof are missing:

- stop in the current command
- say what is missing
- recommend the earlier command that should repair it
- avoid hidden transitions or silent state mutation
