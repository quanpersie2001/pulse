# Pulse Architecture

This document explains Pulse v2 as a single public router skill with a separate runtime CLI.

## In One Sentence

Pulse is a repo-local workflow system where `pulse:workflow` defines user-facing flow, `{{pulse_command}}` reads and coordinates runtime state, and approval gates prevent unsafe execution.

## Mental Model

Pulse has four cooperating layers:

1. **Router contract**: `skills/workflow/SKILL.md` and `skills/workflow/references/*`.
2. **Runtime control plane**: `.pulse/runtime/` for state, handoffs, and reservations.
3. **Workgraph plane**: `.pulse/workgraph/items.jsonl` plus derived views.
4. **Work content plane**: `works/` artifacts and verification evidence.

## Delivery Chain

```mermaid
flowchart TD
    US["/pulse use"] --> EX["/pulse explore"]
    EX -->|Gate 1| PL["/pulse plan"]
    PL -->|Gate 2| VA["/pulse validate"]
    VA -->|Gate 3| SW["/pulse swarm"]
    VA -->|Gate 3| WK["/pulse execute"]
    SW --> WK
    WK --> RV["/pulse review"]
    RV -->|Gate 4| CP["/pulse compound"]
```

## Responsibilities

| Command | Responsibility |
| --- | --- |
| `/pulse use` | Normal session entry; bootstraps readiness when needed and restores context |
| `/pulse onboard` | Explicit bootstrap/remediation entrypoint |
| `/pulse explore` | Lock decisions and context |
| `/pulse brainstorm` | Shape options before planning |
| `/pulse plan` | Select shape and current-work contract |
| `/pulse validate` | Prove feasibility/readiness |
| `/pulse swarm` | Coordinate parallel workers |
| `/pulse execute` | Implement approved work |
| `/pulse review` | Run quality gates and findings |
| `/pulse compound` | Capture reusable learnings |
| `/pulse rescue` | Recover from wrong-shape or stuck execution |
| `/pulse systematic-debug` | Root-cause-first debugging workflow |
| `/pulse note` / `note-distill` | Lightweight capture and synthesis |

## Runtime Boundaries

### User-facing routing

- `/pulse ...` is the only public workflow surface.
- Router behavior is declared in `skills/workflow/SKILL.md`.

### Runtime mutation surface

- `{{pulse_command}} ...` exposes runtime status, readiness, and reservations through the installed workflow skill.
- Canonical metadata source: `.pulse/workgraph/items.jsonl`.

### State and scout

- Canonical runtime state: `.pulse/runtime/*`.
- Scout entrypoint: `{{pulse_command}} status --repo-root <repo> --json`.

## Canonical Planes and Key Files

| Path | Purpose |
| --- | --- |
| `.pulse/runtime/state.json` | machine-readable runtime mirror |
| `.pulse/runtime/STATE.md` | human-readable state narrative |
| `.pulse/runtime/handoffs/manifest.json` | pause/resume index |
| `.pulse/runtime/reservations.json` | active file reservations |
| `.pulse/workgraph/items.jsonl` | canonical work item metadata |
| `.pulse/workgraph/schema.json` | schema contract |
| `.pulse/workgraph/views/*.json` | derived views |
| `.pulse/harness/HARNESS_BACKLOG.md` | materialized backlog template |
| `works/` | content artifacts and verification docs |

## Coordination Model

- Use `{{pulse_command}} ready --repo-root <repo> --json` to surface ready work.
- Use runtime reservations to prevent edit collisions in swarm mode.
- Keep `state.json` and `STATE.md` aligned during transitions.

## What Architecture Intentionally Excludes

This architecture defines `pulse:workflow`, `{{pulse_command}}`, `.pulse/runtime`, and `.pulse/workgraph` as the active operational contract.
