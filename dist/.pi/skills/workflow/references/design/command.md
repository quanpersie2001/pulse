# `pulse:workflow design`

Solution-design command for turning approved direction and discovery evidence into final product/technical decisions before task planning begins.

Design answers:

> What is the approved solution?

`pulse:workflow design` sits after discovery and before planning. It owns all decisions that would otherwise cause planning to invent approach, architecture, schema, API, UX, verification strategy, migration posture, or implementation boundaries.

After design is approved, `pulse:workflow plan` may only decompose the approved solution into tasks and execution packaging. Plan must not change the solution.

## Mission

Produce an approved story-scoped `solution-design.md` that downstream planning can obey without making design decisions.

Design may decide:
- product behavior and interaction details
- technical approach and architecture
- design pattern application
- module/service/interface/API boundaries
- domain model and data/schema shape
- migration, rollout, and compatibility posture
- error handling and fallback behavior
- security/privacy/public contract decisions
- verification strategy at design level
- accepted and rejected alternatives

Design must not:
- create task breakdowns or work items
- assign owners or execution slices
- start implementation
- validate execution readiness
- ignore discovery evidence
- change the approved work direction without explicitly routing back to brainstorm or intake

## Entry criteria

Run `pulse:workflow design` when:

- owning story boundary is confirmed
- `work-brief.md` exists or direction is otherwise explicitly approved
- `discovery.md` exists and is sufficient for solution decisions
- any external research references needed by discovery are available under `references/`

Block design when:

- intake/work boundary is unclear
- direction is missing or contradictory
- discovery lacks evidence needed for material decisions
- the user is asking for task planning rather than solution design

## Required reads

Read in this order when present:

1. story `intake.md`
2. story `work-brief.md`
3. story `discovery.md`
4. story `references/*.md`
5. existing story `solution-design.md` when revising
6. daemon posture from `pulse daemon status`
7. targeted repo files only as needed to clarify discovery evidence, not to redo discovery broadly

## Phase model

### Phase 0 — Design readiness

Verify discovery is sufficient:

- evidence covers the material decision surfaces
- external research is linked where external evidence matters
- contradictions are visible
- open questions blocking design are either resolved now or routed back to explore

If discovery is insufficient, stop and provide exact research questions for `pulse:workflow explore`.

### Phase 1 — Decision inventory

List every decision design must own before planning:

- product/behavior decisions
- technical/architecture decisions
- data/domain decisions
- interface/API decisions
- UX/interaction decisions
- migration/rollout decisions
- verification strategy decisions
- security/privacy/public contract decisions

Do not proceed while a planning-critical decision is implicit.

### Phase 2 — Options and trade-offs

For each major decision surface:

- summarize viable options
- cite discovery evidence and references
- state trade-offs
- reject options that conflict with evidence, scope, or constraints
- identify any decision that still needs user approval

Ask the user one question at a time when a decision cannot be made from evidence and approved direction.

### Phase 3 — Select solution decisions

Select final decisions and assign stable IDs:

```text
D1, D2, D3...
```

Each decision must include:
- decision statement
- rationale
- evidence/source references
- downstream planning constraint

Decisions are binding for plan. If a later command finds a decision wrong or incomplete, it must route back to design.

### Phase 4 — Write `solution-design.md`

Write:

```text
works/<story-id>/solution-design.md
```

Use [solution-design.template.md](./solution-design.template.md) as the required starting structure. Preserve its section order unless a story-specific reason requires an explicit deviation.

Required sections:
- solution summary
- source inputs
- decision log with stable IDs
- accepted approach
- rejected alternatives
- product/behavior design
- technical/architecture design
- data/domain/schema design when relevant
- interface/API/UX design when relevant
- migration/rollout/error/security posture when relevant
- verification strategy
- planning constraints
- out-of-scope/deferred items
- unresolved items, ideally none except explicitly deferred out of scope

### Phase 5 — Design self-review

Review the design for:

- all planning-critical decisions are explicit
- decisions cite discovery evidence or user approval
- no unresolved blockers remain
- no task breakdown or execution owner assignment leaked in
- plan can decompose without changing solution
- rejected alternatives are documented where meaningful
- scope matches `intake.md` and `work-brief.md`

Fix serious issues and rerun once. If serious issues remain, stop for user review or return to explore.

### Phase 6 — User approval and handoff

After self-review, ask the user to approve `solution-design.md`.

After approval:

1. Record workflow posture in the work artifact:
   ```text
   Current: solution design approved for <work>
   Solution design: <works story solution-design.md path>
   Next: invoke pulse:workflow plan
   ```
2. Recommend `pulse:workflow plan` as the next manual command.
3. Do not invoke plan unless the user explicitly asks to continue.

## Plan immutability contract

`solution-design.md` is immutable input for planning unless design is reopened.

Plan may:
- decompose work into tasks
- sequence tasks
- map dependencies
- map evidence and validation work
- prepare execution packaging

Plan must not:
- change approach or architecture
- choose a different design pattern
- revise schema/API/UX/product behavior
- alter migration or verification strategy
- introduce new solution decisions
- silently resolve design gaps

If plan finds a design gap, contradiction, or infeasible decision, it must stop and route back to `pulse:workflow design` or `pulse:workflow explore` with exact repair questions.

## Red flags

Stop if you catch yourself:

- creating tasks or work items
- assigning owners or execution slices
- implementing code
- ignoring discovery evidence
- making decisions without rationale or evidence
- leaving planning-critical choices implicit
- allowing plan to decide solution shape later
- changing the work direction without routing back

## Exit contract

Successful exit requires:
- `solution-design.md` written under the owning story
- stable decision IDs for all material decisions
- evidence-backed rationale for decisions
- explicit planning constraints
- no task breakdown
- explicit user approval request or approval record
- next command recommendation: `pulse:workflow plan` (manual invoke by default)
