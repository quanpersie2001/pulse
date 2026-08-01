# Workflow readiness contract

This document defines an advisory readiness check. It does not create files or
repair state.

## Required evidence

| Check | Evidence or owner | Failure action |
| --- | --- | --- |
| Rust graph exists | `pulse graph validate --repo-root <repo> --json` | Ask before running `pulse graph bootstrap --repo-root <repo> --json` |
| Rust daemon is available | `pulse daemon status` or `pulse daemon doctor` | Ask before running `pulse daemon start` |
| Current work is known | `pulse work list --repo-root <repo> --json` and approved work artifacts | Route to `pulse:workflow plan` or `pulse:workflow use` |
| Current item is executable | `pulse work ready <id> --repo-root <repo> --json` | Route to validation or planning |
| Runtime session is known | `pulse session list` or `pulse session inspect <id>` | Ask for the missing session identifier or route to execution setup |

The supported graph layout is `.pulse/workgraph/nodes/`, `.pulse/workgraph/edges/`,
`.pulse/workgraph/manifest.json`, and `.pulse/workgraph/schemas/`. Generated
projections are outputs of Rust commands, not writable truth.

## Readiness result

Report `PASS`, `DEGRADED`, or `FAIL` with the command output, repository source
commit, active work identifier, daemon posture, and the smallest next action.
Do not invent readiness, runtime, handoff, or reservation state inside the
repository. The skill only reports evidence and recommends commands; it does
not own those mutations.

## Mutation boundary

The workflow may recommend an explicit Rust command after the relevant approval.
Only that command may mutate Pulse state. In particular, the skill must not:

- hand-edit graph node or edge files;
- create or repair schemas, projections, locks, or runtime records itself;
- move repository contents into backup directories or otherwise rewrite the target repository;
- copy legacy state into a new layout; or
- claim that a workflow invocation performed onboarding.

Human-authored plans, design notes, verification notes, and handoff notes may
remain in their owning work-artifact directories. They are not Pulse state.
