# Workgraph Model

This document defines the vocabulary the `pulse:workflow` router uses when it talks about work items and readiness.

## Canonical writable metadata source

The target canonical metadata source is:

```text
.pulse/workgraph/items.jsonl
```

The router talks about work items in a way that matches that model.

## Item kinds

Pulse v2 uses these item kinds:

- `EPIC`
- `STORY`
- `TASK`
- `BUG`

## Status model

Canonical statuses:

- `OPEN`
- `IN_PROGRESS`
- `BLOCKED`
- `CLOSED`

`BLOCKED` is for external blockers.
Dependency blocking is derived state, not a separate canonical status requirement.

## Hierarchy model

Hierarchy and dependency are separate concepts.

- an `EPIC` is the top-level capability boundary
- a `STORY` belongs to an epic
- a `TASK` or `BUG` belongs to a story
- dependencies may exist across epic boundaries

## Ready semantics

A work item is ready when all are true:

- the item is `OPEN`
- it has no external `blocked_reason`
- every dependency is `CLOSED`
- the current workflow has passed the required approval gate for execution

Only dependency edges participate in readiness checks.
Traceability links must not change readiness.

## Dependency vs linked traceability

Pulse uses at least two different relationship meanings between items, and they must not be conflated.

### `depends_on`

Use `depends_on` when one item cannot safely execute or complete until another item is closed.
This relation is blocking.
It affects readiness, execution ordering, and dependency analysis.
Dependency cycles are invalid because they would prevent progress.

### `linked_items`

Use `linked_items` for non-blocking traceability only.
Examples:

- a new story that is a behavior delta over an older closed story
- a maintenance story related to a previous refactor
- parallel work streams that share context but do not gate each other
- follow-up cleanup explicitly separated from the original delivery item

`linked_items` is distinct from `depends_on` in all router behavior:

- it does not affect readiness
- it does not make an item blocked
- it does not participate in dependency cycle detection
- it does not authorize execution ordering by itself
- it exists so humans and tooling can understand historical or semantic relationships

A linked item may also have dependencies, but those are separate edges with separate semantics.
Never upgrade a traceability link into a dependency unless execution really must wait.

## Ownership vs reservation

Pulse v2 deliberately separates durable responsibility from short-lived execution claims.

### Owner

- stored on the canonical item metadata
- expresses durable responsibility
- may name a person, role, or stable agent identity

### Reservation

- stored in runtime state, not canonical item metadata
- expresses the current execution lease
- prevents two workers from doing the same work at once

Do not collapse these concepts into one field.

## Artifacts attached to items

The workgraph points to human-authored work content rather than duplicating it.
Typical attached paths include:

- content path under `works/`
- verification path under `works/`

## Router implications

- `plan` should produce lowercase `plan.md` shapes and materialize approved current-slice items through `node .trae/skills/workflow/scripts/pulse.mjs workgraph`, never by hand-editing `.pulse/workgraph/items.jsonl`.
- `validate` should test whether the Gate 2-approved `plan.md` and materialized TASK/BUG items are ready for execution, using `node .trae/skills/workflow/scripts/pulse.mjs workgraph` output rather than hand-editing `.pulse/workgraph/items.jsonl`.
- `swarm` and `execute` should respect owner and reservation boundaries.
- `review` and `compound` should consume verification and lifecycle evidence rather than redefining metadata.
