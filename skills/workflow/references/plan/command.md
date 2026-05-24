# `pulse:workflow plan`

Task-planning manual for decomposing an approved `solution-design.md` into execution-ready work without changing the solution.

Plan answers:

> How should the approved solution be broken into executable work?

Plan does **not** decide product behavior, technical approach, architecture, schema, API, UX, migration posture, or verification strategy. Those belong to `pulse:workflow design`.

## Mission

Convert approved solution design into one clear task/current-work breakdown that validation can prove and execution can follow.

## Entry criteria

Run `pulse:workflow plan` when:

- `solution-design.md` exists under the owning story
- solution design has explicit user approval or approval posture
- discovery/design/runtime mirrors do not conflict
- the next work is decomposition, sequencing, dependencies, and validation mapping

Block planning when:

- `solution-design.md` is missing, draft, contradictory, or unapproved
- plan would need to choose or change solution approach
- design decisions are missing for schema/API/UX/architecture/verification strategy
- runtime readiness is stale or blocked

## Required inputs

- story `intake.md`
- story `work-brief.md` when brainstorm was used
- story `discovery.md`
- story `references/*.md` when cited by discovery/design
- approved story `solution-design.md` (authoritative)
- `.pulse/memory/critical-patterns.md` when present
- `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md`
- prior planning artifacts if iterating

## Immutable design contract

Plan consumes `solution-design.md` as immutable input.

Plan may:
- decompose work into tasks or current-work slices
- sequence tasks
- identify dependencies and parallelization boundaries
- map validation evidence to tasks
- prepare execution packaging
- recommend execution mode

Plan must not:
- revise design/approach/architecture
- choose a different pattern
- alter schema, API, UX, product behavior, migration, or verification strategy
- add new solution decisions
- silently resolve design gaps

If planning discovers a design gap, contradiction, or infeasible decision, stop and route back to `pulse:workflow design` with exact repair questions. If missing evidence caused the gap, route back to `pulse:workflow explore`.

## Phase model

### Phase 0 — Orientation

Read required inputs and confirm:

- active story boundary
- approved design path/status
- decision IDs and planning constraints
- runtime mirror sync
- prior planning state if any

Hard stop if `solution-design.md` is not the authoritative approved solution.

### Phase 1 — Learnings retrieval

Load critical corrections/ratchets from `.pulse/memory/critical-patterns.md` when present.

Record applied learnings in the planning artifact. If learnings conflict with approved design, stop and route back to design instead of changing the plan silently.

### Phase 2 — Task decomposition

Create or update the story planning artifact, typically:

```text
works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/PLAN.md
```

Break the approved solution into work units.

For each unit:
- objective
- source design decision IDs
- dependencies
- expected touched surfaces at a high level
- acceptance/evidence expectation
- sequencing or parallelization notes
- risks inherited from design

### Phase 3 — Current-work contract

Prepare one bounded current-work contract for the next validation pass:

- entry state
- exit state
- in-scope
- out-of-scope
- dependencies
- validation evidence to produce
- rollback/repair posture if applicable

Do not prepare multiple unrelated slices at once.

### Phase 4 — Gate 2 approval

Present the task breakdown/current-work shape for explicit approval.

Gate 2 approves the plan/decomposition only. It does not approve or revise solution design.

After approval:

1. mark planning artifact approved
2. sync `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json`
3. recommend `pulse:workflow validate`

### Phase 5 — Conditional work item creation

Create execution work items only when the approved planning posture and runtime/workgraph rules allow it.

Do not create future-slice backlog inflation. Create only current-slice items that are ready for validation/execution flow.

## Stop conditions and reroutes

Route to `pulse:workflow design` when:

- a solution decision is missing or contradictory
- design constraints make decomposition impossible
- a safer/different approach appears necessary
- verification strategy needs to change

Route to `pulse:workflow explore` when:

- the design gap is caused by missing evidence
- external/provider/security/domain research is required

Route to `pulse:workflow use` when:

- runtime readiness is untrusted or blocked

## Role boundaries

Plan owns:
- task breakdown
- sequencing
- dependency mapping
- current-work contract
- validation mapping
- execution mode recommendation

Plan does not own:
- solution design
- product/technical decisions
- discovery/deep research
- implementation
- quality signoff

## Handoff posture

At completion, provide:

- approved planning artifact path
- current-work contract path/summary
- item creation posture
- referenced design decision IDs
- recommendation: `pulse:workflow validate`
- default `next_action`: manual invoke

## Red flags

- changing design decisions during planning
- inventing schema/API/UX/architecture in plan
- treating a design gap as a planning detail
- creating tasks not traceable to solution design decisions
- preparing multiple unrelated slices at once
- creating future-slice items prematurely
- vague, non-observable exit criteria
- proceeding beyond Gate 2 without explicit approval

## Exit contract

Successful exit requires:

- approved task/current-work breakdown under `works/`
- every task traces to approved solution design decisions
- no design changes introduced by plan
- one bounded current-work contract
- `.pulse/runtime` mirrors synchronized to artifact truth
- validate-ready handoff

## References

- `planning-reference.md` — task/current-work quality rules
- `work-item-template.md` — canonical execution item schema and normalization contract
