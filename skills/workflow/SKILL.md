---
name: pulse:workflow
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
It does **not** replace the runtime CLI.

- `pulse:workflow ...` chooses the workflow move.
- `pulse-work ...` mutates runtime and workgraph state.

## Setup before routing

Before choosing or executing a command:

1. If repo readiness or runtime posture is unclear, start with [`onboard`](commands/onboard/command.md).
2. If the session is resuming active work, inspect the current runtime and handoff posture before routing onward.
3. When a command is matched, load its command reference before acting.
4. Load shared references whenever gates, workgraph semantics, swarm behavior, verification, or handoff rules matter.

This router is intentionally strict.
Unknown input should not trigger hidden dispatch behavior.

## Commands

| Command | Category | Use when... | Reference | Usually next |
| --- | --- | --- | --- | --- |
| `onboard` | Bootstrap | the repo needs bootstrap, readiness, repair, or migration posture | [commands/onboard/command.md](commands/onboard/command.md) | `explore`, `brainstorm`, `plan` |
| `explore` | Discovery | the design direction is chosen but repo-grounded decisions and constraints still need investigation | [commands/explore/command.md](commands/explore/command.md) | `plan` |
| `brainstorm` | Design | the user goal is real but the feature shape is still open | [commands/brainstorm/command.md](commands/brainstorm/command.md) | `explore` |
| `plan` | Planning | explored context must become a concrete implementation shape | [commands/plan/command.md](commands/plan/command.md) | `validate` |
| `validate` | Readiness | the proposed work needs proof before implementation starts | [commands/validate/command.md](commands/validate/command.md) | `swarm`, `execute`, `plan` |
| `swarm` | Execution | validated work should be executed by multiple agents with explicit coordination | [commands/swarm/command.md](commands/swarm/command.md) | `execute`, `review` |
| `execute` | Execution | a validated work item should be implemented and evidenced | [commands/execute/command.md](commands/execute/command.md) | `review` |
| `review` | Quality | execution is complete and quality evaluation is next | [commands/review/command.md](commands/review/command.md) | `compound`, `execute` |
| `compound` | Learning | the completed cycle should produce reusable learnings | [commands/compound/command.md](commands/compound/command.md) | `plan` |

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
| `pulse-work` | manipulate workgraph items and runtime state once the runtime layer is in place |
| `commands/<command>/command.md` | command-specific behavioral entrypoint |
| `references/shared/*.md` | cross-cutting workflow contracts |

Do not flatten these layers into one file or one command.

## Shared references

Use these references when the active command needs cross-cutting contract detail.

| Concern | Reference |
| --- | --- |
| Workflow pipeline and command responsibilities | [references/shared/workflow-contract.md](references/shared/workflow-contract.md) |
| Plane separation and artifact ownership | [references/shared/planes-and-artifacts.md](references/shared/planes-and-artifacts.md) |
| Work item vocabulary and ready semantics | [references/shared/workgraph-model.md](references/shared/workgraph-model.md) |
| Human approval model | [references/shared/approval-gates.md](references/shared/approval-gates.md) |
| Validation and evidence expectations | [references/shared/verification-contract.md](references/shared/verification-contract.md) |
| Multi-agent orchestration rules | [references/shared/swarm-execution-rules.md](references/shared/swarm-execution-rules.md) |
| Pause, handoff, and resume posture | [references/shared/handoff-and-resume.md](references/shared/handoff-and-resume.md) |
| Harness architecture reference | [references/HARNESS.md](references/HARNESS.md) |
| Harness backlog seed template | [templates/HARNESS_BACKLOG.md](templates/HARNESS_BACKLOG.md) |

## Operating principles

- Keep the public surface as one router, not a growing list of public skills.
- Keep approval gates attached to artifacts and workflow state.
- Keep command-local assets with the command that owns them.
- Keep shared rules in shared references instead of copying them into every command.
- Keep fallback behavior explicit and safe.
