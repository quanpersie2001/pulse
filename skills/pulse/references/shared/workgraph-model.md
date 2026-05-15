# Workgraph Model

This document defines the vocabulary the `/pulse` router uses when it talks about work items and readiness.

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
- a `TASK` or `BUG` belongs to an epic or story
- dependencies may exist across epic boundaries

## Ready semantics

A work item is ready when all are true:

- the item is `OPEN`
- it has no external `blocked_reason`
- every dependency is `CLOSED`
- the current workflow has passed the required approval gate for execution

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

- `plan` should produce shapes that can be turned into workgraph items later.
- `validate` should test whether proposed items are ready for execution.
- `swarm` and `execute` should respect owner and reservation boundaries.
- `review` and `compound` should consume verification and lifecycle evidence rather than redefining metadata.
