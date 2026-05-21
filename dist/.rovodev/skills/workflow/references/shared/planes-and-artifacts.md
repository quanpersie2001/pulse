# Planes and Artifacts

Pulse v2 separates workflow conversation, runtime state, canonical metadata, and human work content into distinct planes.

## Plane model

| Plane | Canonical location | Purpose | What belongs here |
| --- | --- | --- | --- |
| Public router | `skills/workflow/` | user-facing workflow contract | `SKILL.md`, command docs, shared references |
| Runtime plane | `.pulse/runtime/` | machine and operator state for the active workflow | `state.json`, `STATE.md`, handoffs, reservations |
| Workgraph plane | `.pulse/workgraph/` | canonical metadata for work items | `items.jsonl`, `schema.json`, derived views, write lock |
| Work content plane | `works/` | human-authored execution content and verification | epic, story, task, and bug markdown |
| Harness reference/template plane | `skills/workflow/references/`, `skills/workflow/templates/` | source docs and seed artifacts owned by the plugin | `HARNESS.md`, `HARNESS_BACKLOG.md` |

## Why the split matters

Each plane answers a different question:

- router plane -> what workflow move should happen next?
- runtime plane -> what is the current session or gate state?
- workgraph plane -> what items exist and how do they relate?
- work content plane -> what did humans decide, implement, and verify?

Mixing these planes creates drift.

## Canonical source rules

- The router plane owns command behavior and shared workflow language.
- The runtime plane owns active state and resume posture.
- The workgraph plane owns writable item metadata.
- The work content plane owns implementation narratives and verification evidence.
- The harness reference/template plane owns source material that onboarding may materialize later.

## Common mistakes to avoid

### Do not treat references as runtime state

`skills/workflow/references/HARNESS.md` explains the harness contract.
It is not a runtime seed file.

### Do not treat templates as source-of-truth docs

`skills/workflow/templates/HARNESS_BACKLOG.md` is a seed artifact.
It does not replace the harness reference.

### Do not use runtime files as the public contract

Files under `.pulse/runtime/` are operational artifacts.
They do not replace the router contract in `skills/workflow/`.

### Do not put work item metadata in markdown mirrors

Canonical metadata belongs in the workgraph plane, not duplicated through many markdown frontmatters.

## Phase 1 note

In Phase 1, the router and source references are created first.
Later phases will relocate runtime scripts and materialize the target `.pulse/runtime/` and `.pulse/workgraph/` structure behind this contract.
