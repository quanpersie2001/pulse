# `/pulse onboard` Readiness Contract

This document defines the target readiness posture for the single-router Pulse model.

## Core idea

Readiness should tell the user whether Pulse can proceed, in what mode, and what must be repaired first.

## Readiness outcomes

| Outcome | Meaning |
| --- | --- |
| `pass` | the repo is ready for the requested Pulse workflow |
| `degraded` | Pulse can proceed safely, but only with a reduced capability set |
| `blocked` | a required prerequisite is missing or the runtime posture is not safe to trust |

## Core prerequisites

| Capability | Why it matters | Missing effect |
| --- | --- | --- |
| `git` | repo identity, diffing, and normal software work | `blocked` |
| `node` | repo-local Pulse runtime helpers and future `pulse-work` support | `blocked` |
| repo-local Pulse source tree | the router, references, and source-owned assets must exist | `blocked` |
| native swarm capability | only required for `swarm` execution mode | `degraded` to single-worker unless swarm was explicitly required |

## Legacy posture

Legacy tools and artifacts are no longer core readiness requirements in the target model.

Treat these as migration signals instead of baseline blockers:

- `br`
- `bv`
- `.beads/`
- `history/`
- assumptions tied to `preflight` or `using-pulse`

## What readiness should report

A useful readiness brief should include at least:

- current repo root
- overall readiness outcome
- requested mode when known
- recommended mode
- missing core prerequisites
- degraded capabilities
- legacy migration warnings
- recommended next command

## Implementation note

`/pulse onboard` is the operational authority for readiness and runtime maintenance.
Repo-local helper scripts may re-export packaged implementations, but that is packaging detail, not the contract.
The long-term contract is this v2 readiness model under `.pulse/runtime/`.
