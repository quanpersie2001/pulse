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
- `node .codex/skills/workflow/scripts/pulse.mjs ...` reads and coordinates runtime state through the installed workflow skill.

## Setup before routing

Before choosing or executing a command:

1. If repo readiness, runtime posture, or session context is unclear, start with [`use`](references/use/command.md).
2. If the session is resuming active work, let `use` inspect the current runtime and handoff posture before routing onward.
3. When a command is matched, load its command reference before acting.
4. Load shared references whenever gates, workgraph semantics, swarm behavior, verification, or handoff rules matter.

This router is intentionally strict.
Unknown input should not trigger hidden dispatch behavior.

## Commands

| Command | Category | Use when... | Reference | Usually next |
| --- | --- | --- | --- | --- |
| `use` | Session entrypoint | the repo needs readiness/onboarding if stale, session restoration, resume routing, repair, or runtime posture | [references/use/command.md](references/use/command.md) | `explore`, `brainstorm`, `plan` |
| `explore` | Discovery | the design direction is chosen but repo-grounded decisions and constraints still need investigation | [references/explore/command.md](references/explore/command.md) | `plan` |
| `brainstorm` | Design | the user goal is real but the feature shape is still open | [references/brainstorm/command.md](references/brainstorm/command.md) | `explore` |
| `plan` | Planning | explored context must become a concrete implementation shape | [references/plan/command.md](references/plan/command.md) | `validate` |
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
| `node .codex/skills/workflow/scripts/pulse.mjs` | inspect readiness and coordinate reservations through the installed workflow runtime |
| `references/<command>/command.md` | command-specific behavioral entrypoint |
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
