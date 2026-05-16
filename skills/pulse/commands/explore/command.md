# `/pulse explore`

Operational decision-extraction manual for producing an approved, execution-safe `CONTEXT.md` from the canonical story `SPEC.md` artifact under `works/`.

This command is not a lightweight intake summary. It is a gated exploration phase that removes planning-time guesswork by locking behaviorally meaningful decisions.

## Mission

Produce a context artifact that downstream planning can execute against without inventing product behavior, terminology, boundaries, or hidden assumptions.

## Entry criteria

Run `/pulse explore` when all of the following are true:

- the user has an implementation intent (new behavior, changed behavior, or scoped correction)
- decisions that change implementation are still ambiguous
- Gate 1 has not yet been explicitly approved for this feature

Do not run when:

- a stable, approved `CONTEXT.md` already exists for the active slice and no decision drift is present
- the task is pure implementation against Gate 3-approved current work
- runtime readiness is stale or blocked (route to `/pulse onboard`)

## Required reads before questioning

Read in this order (if present):

1. `.pulse/project-docs.json` and the smallest relevant listed docs
2. `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/SPEC.md` for the approved story
3. existing `history/<feature>/CONTEXT.md` (if present)
4. `.pulse/runtime/STATE.md`
5. minimal code scout targets needed to resolve terminology or behavior contradictions

Rule: answer from repo evidence first; ask the user only for decisions evidence cannot settle.

## Phase model (mandatory order)

### Phase 0 — Scope and framing

1. Classify scope: `quick`, `standard`, or `deep`.
2. If unclear, ask one disambiguation question.
3. Detect multi-system requests; split into one foundational system per exploration pass.
4. Optional single step-back framing (one pass only):
   - restate outcome
   - list 2–4 decision axes
   - state what is out of scope for exploration

Stop condition: if the feature cannot be described as one decision surface, do not continue to probing; first narrow scope with the user.

### Phase 1 — Domain classification

Classify affected behavior domains (may be multiple): UI/SEE, API/CALL, runtime/RUN, data/READ, workflow/ORGANIZE.

Purpose: ensure probes target ambiguity that changes implementation behavior.

### Phase 2 — Gray-area extraction

Generate 2–4 gray areas that would force planning assumptions if unresolved.

A valid gray area must:

- influence implementation choices or acceptance behavior
- be absent or contradictory in source inputs
- materially alter scope, verification, or boundary decisions

Filter out:

- library/tool preferences without behavior impact
- architecture ideation
- speculative future capabilities

### Phase 3 — Socratic lock loop (hard gate)

Non-negotiable protocol:

- ask exactly one question per turn
- wait for user response before next question
- prefer single-select options with a recommended default when credible
- use concrete scenario probes for boundary decisions
- resolve terminology conflicts before locking decisions

Locking protocol:

- assign stable IDs: `D1`, `D2`, `D3`...
- after each resolved area, post: `Locking decision Dn: <exact decision>. Confirmed?`
- never renumber previously assigned IDs

Stop conditions (hard):

- if two questions are bundled, reset and re-ask one
- if contradiction remains unresolved, do not continue to artifact writing
- do not proceed until all blocking gray areas are either locked or explicitly deferred

### Phase 4 — Context assembly

Write `history/<feature>/CONTEXT.md` as the single feature source of truth for downstream phases.

Minimum required sections:

- problem outcome and scope boundary
- locked decisions with IDs (`D1...Dn`)
- code/context evidence with concrete paths
- constraints and non-goals
- open questions split into:
  - resolve before planning
  - deferred to planning
- project-doc alignment note (reused terms, corrected terms, missing glossary)

If repeated ambiguity is clearly project-level, add a `Project Docs Follow-up` proposal, but do not modify broader docs unless explicitly requested.

### Phase 5 — Quality check + Gate 1 handoff

Before handoff, verify:

- every implementation-relevant choice is explicit or explicitly deferred
- no locked decision is vague or aesthetic-only
- decision IDs are stable and complete
- no hidden contradictions remain

Then update runtime mirrors truthfully (if touched) and present Gate 1 handoff.

## Gate posture

`/pulse explore` prepares Gate 1 only.

Gate 1 passes only after explicit user approval of `CONTEXT.md`. Until then, downstream shaping is blocked.

## Role boundaries

Explore owns:

- decision extraction
- ambiguity reduction
- context artifact integrity

Explore does not own:

- implementation plans
- workgraph item creation
- architecture design or coding

## Pause/resume and handoff posture

If context is near critical budget or user pauses:

- persist exploration progress in `.pulse/runtime/STATE.md` with active gray area and next single question
- preserve locked decision IDs already confirmed
- resume from the last unresolved decision, not from scratch

Do not mark exploration complete during pause.

## Red flags

- batching unresolved questions
- asking questions repo evidence already answers
- drifting into solution design
- writing `CONTEXT.md` before decisions are locked
- mutating runtime/workgraph state as if Gate 1 already passed

## Exit contract

Successful exit requires:

- `history/<feature>/CONTEXT.md` written and internally consistent
- explicit list of locked decisions (`D1...Dn`)
- Gate 1 approval request
- next command recommendation: `/pulse plan` (manual invoke by default)
