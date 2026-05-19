# `pulse:workflow explore`

Operational decision-extraction manual for turning approved story intent into an execution-safe `CONTEXT.md` grounded in `works/` and current runtime state.

This is a gated exploration phase, not a lightweight summary.

## Mission

Remove planning-time guesswork by locking implementation-relevant decisions with stable IDs and producing a context artifact downstream phases can execute without inventing behavior.

## Entry criteria

Run `pulse:workflow explore` when:

- implementation intent exists
- behavior/constraints are still ambiguous enough to force planner assumptions
- Gate 1 is not yet approved for this active story slice

Do not run when:

- approved context already exists for the exact active story and no decision drift exists
- work is pure implementation under Gate 3-approved current work
- `pulse:workflow use` readiness is stale or blocked

## Required reads before questioning

Read in this order (if present):

1. `.pulse/project-docs.json` and smallest relevant listed docs
2. active story spec: `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/SPEC.md`
3. existing story context: `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/CONTEXT.md`
4. `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json`
5. minimal quick code scout targets required to resolve terminology/behavior contradictions

Quick scout boundary: keep this shallow (targeted grep + 2–3 file reads). Deep codebase analysis belongs to `pulse:workflow plan`.

Active story context is `works/**/CONTEXT.md`; do not route exploration truth through legacy history paths.

Rule: answer from repo evidence first; ask users only for decisions evidence cannot settle.

## Command-local references

- `gray-area-probes.md` — canonical SEE/CALL/RUN/READ/ORGANIZE probe bank
- `context-template.md` — required structure for `works/**/CONTEXT.md`
- `context-reviewer-prompt.md` — optional Phase 4.2 reviewer loop prompt

## Phase model (mandatory order)

### Phase 0 — Scope and framing

1. Classify scope: `quick`, `standard`, or `deep`.
2. If unclear, ask one disambiguation question.
3. Detect multi-system requests; split into one foundational system per exploration pass.
4. Optional one-time step-back framing:
   - restate desired outcome
   - list 2–4 decision axes
   - state what is out of exploration scope

Stop condition: if work cannot be framed as one decision surface, narrow scope before probing.

### Phase 1 — Domain classification

Classify affected behavior domains (can be multiple):

- SEE (UI/presentation)
- CALL (API/integration)
- RUN (runtime/operations)
- READ (data/state)
- ORGANIZE (workflow/ownership)

Purpose: ensure probes target ambiguity that changes implementation behavior.

### Phase 2 — Gray-area extraction

Generate 2–4 gray areas that would force planning assumptions if unresolved.
Use domain-specific probes from `gray-area-probes.md`; select only those genuinely undecided for the active story.

A valid gray area must:

- influence implementation behavior, boundaries, or acceptance criteria
- be absent, contradictory, or overloaded in available sources
- materially alter scope, verification, or dependency decisions

Filter out:

- library/tool preferences without behavior impact
- architecture ideation
- speculative future capabilities

### Phase 3 — Socratic lock loop (hard gate)

Non-negotiable protocol:

- ask exactly one question per turn
- wait for response before next question
- prefer single-select options with a recommended default when credible
- use concrete scenario probes for boundaries/edge cases
- resolve terminology conflicts before locking

Locking protocol:

- assign stable IDs: `D1`, `D2`, `D3`...
- after each resolved area: `Locking decision Dn: <exact decision>. Confirmed?`
- never renumber prior IDs
- after 3–4 questions in one gray area, checkpoint before continuing: `More questions on this area, or move to the next unresolved area?`

Scope discipline during loop:

- if user introduces a new capability outside current boundary, capture it under deferred ideas and return to the active gray area
- if wording conflicts with project docs or code evidence, resolve the contradiction before locking

Stop conditions:

- if multiple questions were bundled, reset and re-ask one
- if contradiction remains unresolved, do not proceed to artifact writing
- do not proceed until every blocking gray area is locked or explicitly deferred

### Phase 4 — Context assembly

Write canonical story context:

- `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/CONTEXT.md`

Populate from `context-template.md`.

Required sections:

- intended outcome and explicit scope boundary
- locked decisions with IDs (`D1...Dn`)
- code/context evidence with concrete paths
- constraints and non-goals
- open questions split into:
  - resolve before planning
  - deferred to planning
- project-doc terminology alignment (reused, corrected, missing)

If repeated ambiguity is clearly project-level, add `Project Docs Follow-up` as a proposal only.

Optional Phase 4.2 reviewer loop:

- run a fresh reviewer pass using `context-reviewer-prompt.md`
- if issues are found, fix and re-run
- after two failed reviewer iterations, ask for direct human review instead of churning

### Phase 5 — Quality check and Gate 1 handoff

Before handoff, verify:

- every implementation-relevant choice is explicit or explicitly deferred
- locked decisions are behaviorally concrete
- decision IDs are complete and stable
- no unresolved contradictions remain

Then, if runtime mirrors are touched, update them truthfully to a pre-approval state and present Gate 1 handoff.

## Gate posture

`pulse:workflow explore` prepares Gate 1 only.

Gate 1 passes only after explicit user approval of `works/**/CONTEXT.md`. Until approved, shaping/execution commands are blocked.

## Role boundaries

Explore owns:

- ambiguity reduction and decision locking
- context artifact integrity

Explore does not own:

- implementation planning
- work item execution
- architecture/coding

## Pause/resume posture

If pausing:

- persist current exploration posture in `.pulse/runtime/STATE.md` (active gray area + next single question)
- preserve already confirmed decision IDs
- resume from the next unresolved decision, not from scratch

Do not mark exploration complete while paused.

## Red flags

- batching unresolved questions
- asking questions repo evidence already answers
- drifting into solution design
- writing `CONTEXT.md` before decisions are locked
- mutating runtime/workgraph state as if Gate 1 already passed
- writing active truth to `history/`, `.beads`, or legacy `pulse:*` skill naming instead of `works/**` + `pulse:workflow` routing

## Exit contract

Successful exit requires:

- `works/**/CONTEXT.md` written and internally consistent
- explicit locked decision list (`D1...Dn`)
- explicit Gate 1 approval request
- no planning/execution actions taken before Gate 1 approval
- next command recommendation: `pulse:workflow plan` (manual invoke by default)
