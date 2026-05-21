# Runtime Adapter Spec (Swarm)

Swarm behavior is canonical; adapter primitives may vary.

## Required capabilities

Adapter must support:

- bounded worker spawn
- coordinator-to-worker follow-ups
- worker-to-coordinator event reports
- stable runtime identity per worker turn

## Mapping guidance

### Claude Code

- spawn workers with Agent
- coordinate through SendMessage
- optional Task metadata only; work truth remains in `.pulse/workgraph/items.jsonl`

### Codex

- spawn native subagents
- parent thread is coordination surface
- keep state truth in `.pulse/runtime/*` and `.pulse/workgraph/*`

## Invariants across adapters

- ready work selected from `node .pi/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json`
- graph truth from `.pulse/workgraph/views/graph.json`
- reservation layer from `node .pi/skills/workflow/scripts/pulse.mjs reservation ... --repo-root <repo> --json`
- owner-scoped handoffs in `.pulse/runtime/handoffs/`
- one shared-branch commit slot at a time
