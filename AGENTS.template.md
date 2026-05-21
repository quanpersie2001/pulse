<!-- PULSE:START -->
# Pulse Workflow

Use `/pulse onboard` first in this repo unless you are resuming an approved handoff.

## What Pulse Is / Is Not

Pulse is a validate-first, docs-first workflow router for Claude Code and Codex.
Pulse is not a license to skip decision locking, validation, review gates, or human approval.

## One-Line Glossary

- context artifact — locked decisions downstream work must honor.
- shape artifact — approved execution shape (`work-shape.md`, `phase-plan.md`, or `epic-map.md`).
- current-work artifact — execution-ready slice contract.
- handoff — pause/resume contract for the next actor.
- `{{pulse_command}} status` — read-only scout for current workflow state.
- work item — one unit in the canonical workgraph.

## Startup

1. Read this file at session start and again after context compaction.
2. If runtime readiness is missing/stale in `.pulse/runtime/tooling-status.json`, run `/pulse onboard`.
3. Run `{{pulse_command}} status --repo-root <repo> --json` for scout state.
4. If `.pulse/runtime/handoffs/manifest.json` exists, surface it and wait for explicit resume confirmation.
5. If `.pulse/memory/critical-patterns.md` exists, read it before planning or execution.

## Chain

```
/pulse onboard
  → /pulse explore
  → /pulse plan
  → /pulse validate
  → /pulse swarm or /pulse execute
  → /pulse review
  → /pulse compound
```

## Critical Rules

1. Never execute without validate approval.
2. Locked context decisions are source-of-truth for downstream work.
3. If context usage passes roughly 65%, write a handoff and pause cleanly.
4. Treat `.pulse/runtime/state.json` as the machine mirror and `.pulse/runtime/STATE.md` as the human narrative; keep them aligned.
5. After compaction, re-run scout and re-open handoff + runtime state before continuing.
6. P1 review findings block merge.

## Runtime Planes

1. **Control plane — `.pulse/runtime/`**: state, handoffs, reservations.
2. **Workgraph plane — `.pulse/workgraph/`**: canonical metadata and views.
3. **Work content plane — `works/`**: implementation artifacts and verification evidence.

## Working Files

```
.pulse/runtime/
  tooling-status.json
  state.json
  STATE.md
  handoffs/manifest.json
  reservations.json

.pulse/workgraph/
  items.jsonl
  schema.json
  views/

works/
  epics/
```

## Operator Cookbook

### Startup scout

1. Run `/pulse onboard` when runtime readiness is missing/stale.
2. Run `{{pulse_command}} status --repo-root <repo> --json`.
3. Open only artifacts indicated by the scout.

### Resume scout

- Surface `.pulse/runtime/handoffs/manifest.json` if present.
- Re-open handoff + `.pulse/runtime/state.json` + `.pulse/runtime/STATE.md`.
- If state sources disagree, surface mismatch instead of guessing.

### Swarm vs single-worker

- Use swarm when current approved work has enough parallelizable items.
- Use single-worker when coordination overhead is unnecessary.
- Gate 3 approval is required for both.

## Session Finish

Before ending a substantial work chunk:

1. Update/close the active work item.
2. Leave runtime state and handoff files consistent.
3. Surface remaining blockers and next actions.
<!-- PULSE:END -->
