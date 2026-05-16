# `pulse:workflow plan`

Operational shaping manual for converting approved exploration context into one approved execution shape and one bounded current-work contract.

This is where strategy becomes executable structure. It is not a short synthesis memo.

## Mission

Choose the least workflow that safely fits reality, produce shape artifacts that survive validation scrutiny, and prepare exactly one current slice for Gate 3 validation.

## Entry criteria

Run `pulse:workflow plan` when:

- Gate 1 context is approved
- story context artifacts under `works/` exist and are stable
- implementation must be shaped into bounded execution work

Block planning when:

- context is missing or unapproved
- onboarding/readiness is stale or blocked
- active shape and runtime mirrors disagree and cannot be reconciled from artifact truth

## Required inputs

- story-scoped context artifact under `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/CONTEXT.md` (authoritative)
- story-scoped spec artifact under `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/SPEC.md`
- `.pulse/project-docs.json` and minimal relevant project docs
- `.pulse/memory/critical-patterns.md`
- `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md` for mirror checks
- prior planning artifacts if iterating in the same story directory (canonical: `DISCOVERY.md`, `APPROACH.md`, and one approved shape doc)

## Core model

```text
Mode -> Shape -> Current Work -> (Conditional Workgraph Items) -> Validate
```

Modes:

- `direct_task`
- `spike`
- `small_change`
- `standard_feature`
- `high_risk_feature`

Shapes:

- `work-shape.md` (direct/spike/small)
- `phase-plan.md` (milestone sequencing)
- `epic-map.md` (capability/risk decomposition)

## Phase model (mandatory order)

### Phase 0 — Learnings retrieval

1. Load critical corrections/ratchets from `.pulse/memory/critical-patterns.md`.
2. Record applied learnings in the story planning discovery artifact under `works/`.
3. If memory appears stale, warn but continue with an explicit caution note.

Stop condition: do not proceed to discovery while known prior failures for similar work remain unapplied.

### Phase 1 — Discovery

Create `DISCOVERY.md` in the active story directory under `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/` with practical implementation landscape:

- existing architecture surfaces
- relevant module ownership and interfaces
- constraints and dependency realities
- external references only when truly novel

Depth rule: enough to justify mode/shape decisions, not broad research for its own sake.

### Phase 2 — Synthesis

Create `APPROACH.md` in the active story directory under `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/`:

- recommended path and why it fits locked decisions
- rejected alternatives and reasons
- explicit risk map (especially HIGH risk)
- verification posture and boundary-preservation strategy

For each HIGH-risk component, define:

- risk statement
- validating owner
- decisive YES/NO feasibility question
- downstream impact if YES vs NO

### Phase 3 — Mode gate + shape draft (Gate 2 setup)

1. Choose mode first and explain why lighter modes are insufficient when applicable.
2. Choose exactly one shape artifact and set `Approval status: PENDING`.
3. Stop for explicit Gate 2 approval.

Hard stop: no current-work prep is authoritative before Gate 2 approval.

### Phase 4 — Gate 2 sync + current-work prep

After explicit approval:

1. Mark approved state in the shape artifact.
2. Sync `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json` to the same approved shape and current slice.
3. Prepare one bounded current-work contract only:
   - entry state
   - exit state
   - in-scope
   - out-of-scope
   - verification contract
   - dependency edges

### Phase 5 — Conditional work item creation

Planning creates execution work items only when allowed by mode/readiness posture.

Allowed immediately:

- `direct_task`
- already-proven `small_change`

Deferred (feasibility-first):

- `standard_feature`
- `high_risk_feature`

When validate returns `ready` or `ready-with-constraints` and requests items, create only approved current-slice items, not future slices.

Store execution metadata in `.pulse/workgraph/items.jsonl` through the runtime tools, and keep human-facing story mapping under `works/`.

## Gate posture

`pulse:workflow plan` prepares and records Gate 2.

Gate 2 authorizes current-work prep inside planning. It does not auto-start validation unless the user explicitly asks to continue now.

## Stop conditions and reroutes

Route back to `pulse:workflow explore` when:

- context decisions are contradictory or under-specified

Route to `pulse:workflow onboard` when:

- runtime readiness is untrusted/blocked

Pause and request approval when:

- shape changes materially after prior approval

## Role boundaries

Plan owns:

- mode selection
- shape artifact quality
- current-work contract clarity

Plan does not own:

- execution
- quality signoff
- speculative future-slice backlog inflation

## Handoff posture

At completion, provide:

- approved shape artifact path (under `works/`)
- prepared current-work artifact path(s)
- explicit item-creation posture (present/absent by design)
- recommendation: `pulse:workflow validate`
- default `next_action`: manual invoke

## Red flags

- skipping mode justification
- defaulting to one shape without fit rationale
- preparing multiple slices at once
- creating future-slice items prematurely
- vague, non-observable exit criteria
- proceeding beyond Gate 2 without explicit approval

## Exit contract

Successful exit requires:

- discovery and approach artifacts written under `works/`
- one approved shape artifact under `works/`
- one bounded current-work contract
- `.pulse/runtime` mirrors synchronized to artifact truth
- validate-ready handoff

## References

- `references/planning-reference.md` — mode-quality rules and shape/current-work templates
- `references/work-item-template.md` — canonical execution item schema and normalization contract
