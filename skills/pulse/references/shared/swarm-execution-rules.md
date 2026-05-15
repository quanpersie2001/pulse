# Swarm Execution Rules

Use this contract when `/pulse swarm` is the right execution mode.

## Swarm prerequisites

Swarm execution requires all of the following:

- the work has passed Gate 3
- the work slice can be decomposed into clear parallel boundaries
- worker ownership and handoff rules are explicit
- verification boundaries are explicit
- the runtime can coordinate reservations safely

If any prerequisite is missing, prefer `execute` or route back to `plan`.

## Coordinator responsibilities

The coordinator owns:

- decomposition into worker-sized slices
- assignment of ownership boundaries
- file or item reservation policy
- conflict resolution
- handoff routing
- commit queue discipline when multiple workers share one branch

The coordinator does not skip review on behalf of workers.

## Worker responsibilities

Each worker should:

- stay inside its assigned boundary
- read the active contract before editing
- claim the required reservation before mutating shared scope
- report blockers quickly
- produce verification evidence for its slice
- hand off cleanly when context or ownership changes

## Ownership and reservation rules

- durable `owner` describes responsibility
- runtime reservation describes the active lease
- a worker must not edit around a reservation conflict
- reservations should be released or handed off explicitly

## Shared-branch rule

When the swarm shares one branch, commit-producing steps must be serialized through a single commit queue.

Do not let multiple workers race commits on the same branch.

## Review boundary

Execution and review stay separate.

- workers execute
- reviewers evaluate
- a completed worker slice still passes through `review`

## Failure behavior

If a swarm run becomes unsafe or too entangled:

- stop launching more workers
- collapse back to a narrower plan or single-worker execution
- use `rescue` when the decomposition itself is the problem

## Router implications

`swarm` is not a standalone product surface.
It is an execution mode inside the `/pulse` workflow pipeline.
