# AGENTS.md — Pulse Operator Contract

Read this file at every session start. Re-read after context compaction.

## What is Pulse?

Pulse is a validate-first, docs-first workflow system with a single public skill router: **`pulse:workflow`**. Runtime reads and reservations use the rendered **`{{pulse_command}}`** command from the installed workflow skill.

## What Pulse Is / Is Not

Pulse is:

- a gated workflow with explicit human approvals and repo artifacts
- a skill plugin for Claude Code and Codex
- compatible with swarm and single-worker execution

Pulse is not:

- permission to skip locked context, validating, or review gates
- a fragmented public workflow surface
- a replacement for human gate approval

## One-Line Glossary

- context artifact — locked decisions downstream work must honor.
- shape artifact — approved planning shape (`work-shape.md`, `phase-plan.md`, `epic-map.md`).
- current-work artifact — execution-ready contract for active delivery slice.
- work item — one unit of execution in the canonical workgraph.
- handoff — pause/resume contract for the next actor.
- `{{pulse_command}} status` — read-only runtime scout.

## Public Router Commands

| Command | Purpose |
|---|---|
| `pulse:workflow use` | Session entrypoint, readiness, and resume context |
| `pulse:workflow explore` | Discover and lock decisions |
| `pulse:workflow brainstorm` | Expand and compare approaches |
| `pulse:workflow plan` | Select shape and execution contract |
| `pulse:workflow validate` | Prove feasibility/readiness |
| `pulse:workflow swarm` | Coordinate parallel execution |
| `pulse:workflow execute` | Implement approved work |
| `pulse:workflow review` | Enforce quality gates |
| `pulse:workflow compound` | Capture durable learnings |

## Chain

```
pulse:workflow use → pulse:workflow brainstorm (optional) → pulse:workflow explore → pulse:workflow plan → pulse:workflow validate → pulse:workflow swarm or pulse:workflow execute → pulse:workflow review → pulse:workflow compound
```

## Go Mode Gates

- **GATE 1** (after explore): approve locked context.
- **GATE 2** (after plan): approve shape artifact.
- **GATE 3** (after validate): approve feasibility-validated current work before execution.
- **GATE 4** (after review): P1 findings must be fixed before merge.

## Core Runtime Tools

- `pulse:workflow` — user-facing workflow router
- `{{pulse_command}} status --repo-root <repo> --json` — scout orientation
- `{{pulse_command}} ready --repo-root <repo> --json` — ready work inspection
- `{{pulse_command}} reservation ... --repo-root <repo> --json` — reservation coordination
- native swarm adapters — Claude teammates/Codex subagents

## Packaged Standalone Utility Skills

- `architecture-rescue`
- `systematic-debug-fix`
- `dev-note`
- `dev-note-distil`
- `prompt-leverage`

## 3-Plane Model

1. **Control plane — `.pulse/runtime/`**: state, handoffs, reservations, runtime mirrors.
2. **Workgraph plane — `.pulse/workgraph/`**: canonical metadata and derived views.
3. **Work content plane — `works/`**: epics/stories/tasks/bugs and verification artifacts.

## File Conventions

```
.pulse/runtime/tooling-status.json
.pulse/runtime/state.json
.pulse/runtime/STATE.md
.pulse/runtime/handoffs/manifest.json
.pulse/runtime/reservations.json
.pulse/workgraph/items.jsonl
.pulse/workgraph/schema.json
.pulse/workgraph/views/
.pulse/harness/HARNESS_BACKLOG.md
works/
```

## Critical Rules

1. Never execute without validating approval.
2. Locked context decisions are source-of-truth for downstream work.
3. If context usage exceeds ~65%, write a handoff and pause cleanly.
4. Keep `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md` aligned.
5. After compaction, re-read this file, run scout, then reopen handoff + runtime state before continuing.
6. P1 review findings always block merge.
7. Do not edit `dist/` generated skill outputs directly; update source files under `skills/` and regenerate with `scripts/build-skills.mjs`.

## Operator Cookbook

### Start a fresh run

1. Run `pulse:workflow use`.
2. Run `{{pulse_command}} status --repo-root <repo> --json`.
3. Open only the artifacts scout points to.

### Resume safely

- Surface `.pulse/runtime/handoffs/manifest.json` before resume.
- Wait for explicit resume confirmation.
- Rehydrate from handoff, then verify runtime mirrors.

### Pick swarm vs single-worker

- Use swarm only when approved work has enough parallelizable items.
- Use single-worker when Pulse discipline is needed but parallelism is not.
- Gate 3 still applies to both.

### Runtime workgraph quick reads

```bash
{{pulse_command}} ready --repo-root <repo> --json
{{pulse_command}} reservation list --repo-root <repo> --active-only --json
```

## Landing the Plane (Session Completion)

Before ending a substantial work chunk:

1. Update or close active work items.
2. Leave `.pulse/runtime/state.json`, `.pulse/runtime/STATE.md`, and handoff files consistent.
3. Capture unresolved blockers and next actions.
4. Commit and push code changes through normal Git flow.

## Optional Session Search and Memory Tools

CASS (`cass`) and cass-memory (`cm`) are optional accelerators for transcript search and recall. Treat current repo artifacts as source-of-truth when discrepancies appear.

