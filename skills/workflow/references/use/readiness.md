# `pulse:workflow use` Readiness Contract

This document defines the readiness posture `use` must enforce before session loading and downstream workflow routing.

## Core idea

Readiness must tell the operator whether Pulse can proceed, in what mode, and what must be repaired first. Once readiness is established, `use` continues into session loading.

## Readiness outcomes

| Outcome | Meaning |
| --- | --- |
| `PASS` | Ready for the requested Pulse workflow posture. |
| `DEGRADED` | Safe to proceed only with an explicit reduced capability. |
| `FAIL` | Required prerequisite is missing or runtime posture is unsafe. |

## Core prerequisites

| Capability | Why it matters | Missing effect |
| --- | --- | --- |
| `git` | Repo identity and normal software workflow operations | `FAIL` when source-control context is required |
| `node` | Plugin-owned Pulse runtime helpers | `FAIL` |
| Pulse workflow source tree | Router, references, templates, and runtime-owned assets must be available through the installed skill package | `FAIL` |
| installed `pulse-work` surface | Runtime and workgraph mutation commands must be available locally | `FAIL` after materialization is requested |
| valid workgraph schema | `.pulse/workgraph/schema.json` is the machine-readable workgraph contract | `FAIL` |
| valid workgraph metadata | `.pulse/workgraph/items.jsonl` is the only writable item metadata truth | `FAIL` |
| safe session pointers | Session load must not read outside the repo or allowed roots | `FAIL` or rejected path |
| verified swarm capability | Required only for `swarm` execution posture | `DEGRADED` to `execute` unless swarm is explicitly required |

## Required v2 runtime files

Use must verify or materialize:

```text
.pulse/runtime/tooling-status.json
.pulse/runtime/state.json
.pulse/runtime/STATE.md
.pulse/runtime/handoffs/manifest.json
.pulse/runtime/reservations.json
```

## Required v2 workgraph files

Use must verify or materialize:

```text
.pulse/workgraph/items.jsonl
.pulse/workgraph/schema.json
.pulse/workgraph/views/active.json
.pulse/workgraph/views/closed.json
.pulse/workgraph/views/ready.json
.pulse/workgraph/views/graph.json
```

`write.lock` may be absent when no mutation is active. If it exists and is owned by a live process, readiness must fail for mutating workflow steps.

## Plugin runtime availability

Use must verify that runtime helpers are available from the installed workflow skill package or `pulse-work` surface. Canonical runtime code is not a repo-local readiness asset.

`.pulse/scripts/` may exist only as an optional compatibility shim surface. Missing shims must not block a greenfield v2 repo; stale shims are warnings unless an operator is actively treating them as current truth.

## Harness readiness

Use must verify or materialize:

```text
.pulse/harness/HARNESS_BACKLOG.md
```

The source template is `skills/workflow/templates/HARNESS_BACKLOG.md`.

`skills/workflow/references/HARNESS.md` is the canonical harness operating reference. A runtime `.pulse/harness/HARNESS.md` must not become a second source of truth.

## Session-load readiness

Use must be able to report:

- session posture: `fresh`, `resumable`, `active`, or `conflicted`
- active command when known
- active epic, story, and work item IDs when known
- selected handoff when one owner is selected
- resume options when multiple owners are available
- read-first paths derived from the selected handoff and workgraph metadata
- loaded files, missing files, rejected paths, and conflicts
- recommended next workflow command

Session-load reads must be pointer-driven. Use must not recursively load all of `works/`, `docs/`, `.pulse/memory/`, or `.pulse/runtime/`.

Allowed session-read roots:

```text
AGENTS.md
.pulse/runtime/handoffs/
.pulse/memory/
works/
docs/
```

## What readiness must report

A complete readiness brief should include:

- repo root
- readiness outcome (`PASS`/`DEGRADED`/`FAIL`)
- requested mode when known
- recommended mode
- active command when known
- active epic, story, and work item IDs when known
- session-load summary
- open reservations
- resumable handoffs
- missing core prerequisites
- degraded capabilities
- domain status for `.pulse`, `docs`, and `works`
- runtime file status
- workgraph file status
- plugin runtime availability and optional shim warnings
- harness backlog status
- domain normalization status for `.pulse`, `docs`, and `works` (`missing|compliant|non_compliant`)
- backup paths and onboarding reconstruction briefs when semantic reconstruction is required
- loaded, missing, and rejected session files
- recommended next command

## Contract boundary

`pulse:workflow use` is the operational authority for readiness and session loading.

Runtime helper implementation can evolve, but this readiness contract remains stable at the workflow surface. Downstream commands must consume the result rather than recreating a separate readiness or session-load contract.
