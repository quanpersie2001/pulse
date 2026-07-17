---
name: workflow
description: >-
  Single public Pulse router. Use when the user wants to run a Pulse workflow
  through `pulse:workflow <command>`.
metadata:
  version: '0.3'
  ecosystem: pulse
  dependencies: []
---

# `pulse:workflow`

`pulse:workflow` is the single public workflow entrypoint for Pulse.

It owns command routing for the Pulse workflow surface.
It does **not** replace the rendered runtime command.

- `pulse:workflow ...` chooses the workflow move.
- `node .gemini/skills/workflow/scripts/pulse.mjs ...` reads and coordinates runtime state through the installed workflow skill.

## Setup before routing

Before choosing or executing a command:

1. If repo readiness, runtime posture, or session context is unclear, start with [use](references/use/command.md).
2. If the session is resuming active work, let `use` inspect the current runtime and handoff posture before routing onward.
3. When a command is matched, load its command reference before acting.
4. Load command-local or shared references only when the active section points to them, for example workgraph semantics, verification, handoff rules, or swarm coordination details.

This router is intentionally strict.
Unknown input should not trigger hidden dispatch behavior.

## Commands

| Command | Category | Use when... | Reference | Usually next |
| --- | --- | --- | --- | --- |
| `use` | Session entrypoint | the repo needs readiness/onboarding if stale, session restoration, resume routing, repair, or runtime posture | [references/use/command.md](references/use/command.md) | `intake`, `brainstorm`, `explore`, `design`, `plan` |
| `intake` | New-work admission | `use` reports an empty session and the user has new input to classify before direction, discovery, design, or planning | [references/intake/command.md](references/intake/command.md) | `brainstorm`, `explore`, `design` |
| `brainstorm` | Direction | the user goal is real but the work direction is still open | [references/brainstorm/command.md](references/brainstorm/command.md) | `explore` |
| `explore` | Discovery | approved direction needs repo/domain/external evidence before solution decisions | [references/explore/command.md](references/explore/command.md) | `design` |
| `design` | Solution design | discovery evidence must become final product/technical/solution decisions before task planning | [references/design/command.md](references/design/command.md) | `plan` |
| `plan` | Task planning | approved solution design must be decomposed into executable work | [references/plan/command.md](references/plan/command.md) | `validate` |
| `validate` | Readiness | the proposed work needs proof before implementation starts | [references/validate/command.md](references/validate/command.md) | `swarm`, `execute`, `plan` |
| `swarm` | Execution | validated work should be executed by multiple agents with explicit coordination | [references/swarm/command.md](references/swarm/command.md) | `execute`, `review` |
| `execute` | Execution | a validated work item should be implemented and evidenced | [references/execute/command.md](references/execute/command.md) | `review` |
| `review` | Quality | execution is complete and quality evaluation is next | [references/review/command.md](references/review/command.md) | `compound`, `execute` |
| `compound` | Learning | the completed cycle should produce reusable learnings | [references/compound/command.md](references/compound/command.md) | `plan` |

## Routing rules

1. **No command provided**: render the command table as the help surface, explain what each command owns, and suggest likely next commands from user intent when obvious.
2. **First word matches a command**: load that command reference and treat the remaining text as the user's work context.
3. **First word does not match a command**: stay inside the router, render help again, and suggest likely commands from intent.

## Router boundary

The router owns conversational workflow selection.
The runtime owns canonical mutable state.

| Surface | Responsibility |
| --- | --- |
| `pulse:workflow` | choose the workflow move, load command guidance, preserve gate discipline |
| `node .gemini/skills/workflow/scripts/pulse.mjs` | inspect readiness and coordinate reservations through the installed workflow runtime |
| `references/<command>/command.md` | command-specific behavioral entrypoint |
| `references/shared/*.md` | cross-cutting workflow contracts |

Do not flatten these layers into one file or one command.

## Approval gates

Pulse keeps human approval attached to artifacts and runtime state.

`intake` is pre-gate admission: it may classify or confirm a boundary package, but it never approves direction, solution, plan, execution, or review.

| Gate | When it happens | What gets approved | If not approved | Default next |
| --- | --- | --- | --- | --- |
| Direction approval | after `brainstorm` when used | `work-brief.md` direction, scope, constraints | stay in `brainstorm` | `explore` |
| Gate 1 | after `design` | `solution-design.md` final product/technical/solution decisions | stay in `design` or return to `explore` | `plan` |
| Gate 2 | after `plan` | task breakdown/current-work shape derived from approved design | stay in `plan` or return to `design` | `validate` |
| Gate 3 | after `validate` | current execution slice is feasible and safe to start | do not execute | `swarm` or `execute` |
| Gate 4 | after `review` | completed change is acceptable to merge or ship | fix findings before approval | `compound` |

Gate rules:

- A gate must never be marked approved without explicit user sign-off.
- Runtime state should record the current gate, gate status, active command, recommended next command, and next action when a gate is approved or pending.
- `brainstorm` may ask for direction approval, but it does not approve solution design.
- `explore` produces evidence, but it does not approve final solution design.
- `design` prepares Gate 1; it does not auto-approve it.
- `plan` prepares Gate 2; it must not change approved design.
- `validate` prepares Gate 3; it does not auto-start implementation.
- `review` prepares Gate 4; it does not auto-merge or ship.
- P1 review findings block Gate 4 approval until fixed.

## Shared references

Use shared references only for cross-cutting contracts that multiple commands must interpret consistently. Prefer command-local references for command-specific behavior.

| Concern | Reference |
| --- | --- |
| Work item vocabulary and ready semantics | [references/shared/workgraph-model.md](references/shared/workgraph-model.md) |
| Pause, handoff, and resume posture | [references/shared/handoff-and-resume.md](references/shared/handoff-and-resume.md) |
| Harness architecture reference | [references/HARNESS.md](references/HARNESS.md) |
| Harness backlog seed template | [templates/HARNESS_BACKLOG.md](templates/HARNESS_BACKLOG.md) |

Command-local references include behavior that belongs to one command, for example [swarm event protocol](references/swarm/runtime-appendix.md).

## Operating principles

- Keep the public surface as one router, not a growing list of public skills.
- Keep approval gates attached to artifacts and workflow state.
- Keep command-local assets with the command that owns them.
- Keep shared rules in shared references instead of copying them into every command.
- Keep fallback behavior explicit and safe.
