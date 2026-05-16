# `pulse:workflow` Workflow Contract

This document describes the Pulse workflow pipeline in router language.

## Router contract

`pulse:workflow` is the only public workflow surface.

- `pulse:workflow` renders help.
- `pulse:workflow <command>` loads that command's contract.
- Unknown commands fall back to help.
- The router never dispatches silently to legacy skills.

## Canonical command pipeline

```text
pulse:workflow onboard
  -> pulse:workflow brainstorm     (optional when feature shape is still vague)
  -> pulse:workflow explore
  -> pulse:workflow plan
  -> pulse:workflow validate
  -> pulse:workflow swarm | pulse:workflow execute
  -> pulse:workflow review
  -> pulse:workflow compound
```

Intervention commands may be entered when needed:

- `pulse:workflow rescue`
- `pulse:workflow systematic-debug`
- `pulse:workflow note`
- `pulse:workflow note-distill`

## Command responsibilities

| Command | Primary responsibility |
| --- | --- |
| `onboard` | bootstrap the repo, report readiness, and surface migration posture |
| `explore` | understand the codebase, current state, and implementation-relevant decision context |
| `brainstorm` | shape vague intent into candidate approaches and an approved design before exploration |
| `plan` | turn context into a concrete implementation shape |
| `validate` | prove the shape is executable and expose risk before work starts |
| `swarm` | orchestrate validated multi-agent execution |
| `execute` | implement a validated work item |
| `review` | evaluate completed changes and enforce the final quality gate |
| `compound` | capture reusable learning after the cycle |
| `rescue` | recover from wrong-shape or dead-end execution paths |
| `systematic-debug` | investigate bugs with explicit evidence and hypothesis narrowing |
| `note` | capture tactical decisions and breadcrumbs |
| `note-distill` | synthesize raw notes into reusable guidance |

## Gated progression

The router keeps the existing human-gate model, but the gates attach to artifacts rather than legacy skill names.

1. after `explore`, approve the context artifact
2. after `plan`, approve the selected shape artifact
3. after `validate`, explicitly approve execution
4. after `review`, resolve blocking findings before merge or ship approval

See `approval-gates.md` for the gate details.

## Router vs runtime boundary

Use the router to decide **what workflow move should happen next**.

Use `pulse-work` to mutate canonical workgraph state once the runtime exists.

Examples:

- `pulse:workflow plan` decides the shape of work.
- `pulse-work create` will eventually create work items in the workgraph.

The router is conversational and decision-oriented.
The runtime is mechanical and state-oriented.

## Legacy mapping

| Legacy surface | Router replacement |
| --- | --- |
| `preflight` + `using-pulse` | `onboard` |
| `exploring` | `explore` |
| `brainstorming` | `brainstorm` |
| `planning` | `plan` |
| `validating` | `validate` |
| `swarming` | `swarm` |
| `executing` | `execute` |
| `reviewing` | `review` |
| `compounding` | `compound` |
| `architecture-rescue` | `rescue` |
| `systematic-debug-fix` | `systematic-debug` |
| `dev-note` | `note` |
| `dev-note-distil` | `note-distill` |
| `dream` | removed |

## Failure posture

If a command cannot proceed because required context, approvals, or proof are missing:

- stop in the current command
- say what is missing
- recommend the earlier command that should repair it
- avoid hidden transitions or silent state mutation
