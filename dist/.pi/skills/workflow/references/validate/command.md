# `pulse:workflow validate`

Readiness command for proving an approved story `plan.md` and its current-slice TASK/BUG work items are executable under real repository constraints before implementation begins.

Validate answers:

> Is this approved execution slice feasible, coherent, and safe to start now?

Validate enforces Gate 3. A `ready` decision is not execution approval until the user explicitly approves Gate 3.

## Mission

Produce a precise execution-readiness decision for the approved current slice, expose blockers with exact reroutes, recommend `swarm` or `execute`, and stop for explicit Gate 3 approval.

Validate must prove:

- Gate 2 approved `plan.md` exists and is the active planning artifact
- approved TASK/BUG materialization required by the plan is complete
- runtime mirrors, story artifacts, and workgraph metadata point to the same active slice
- planned work still fits the current repository reality
- high-impact assumptions are proven or routed to a bounded probe/spike
- TASK/BUG contracts contain enough file scope, dependency, verification, and evidence detail for execution

## Entry criteria

Run `pulse:workflow validate` when:

- `pulse:workflow plan` has completed Gate 2 approval
- story `plan.md` is approved and current
- approved TASK/BUG work items have been materialized through `node .pi/skills/workflow/scripts/pulse.mjs workgraph` when the plan requires them
- execution has not started for the selected slice
- runtime/workgraph posture can identify the active epic/story and current slice

Do not run when:

- Gate 2 approval is pending, ambiguous, rejected, or not recorded
- `plan.md`, runtime mirrors, and workgraph metadata disagree about active work
- approved TASK/BUG materialization from `plan.md` is incomplete
- onboarding/readiness is stale or blocked
- the user is asking to implement, fix, review, or merge rather than prove readiness

If entry criteria fail, route precisely:

- missing or unapproved `plan.md` → `pulse:workflow plan`
- approved design gap discovered while validating → `pulse:workflow design`
- missing discovery/evidence needed to decide feasibility → `pulse:workflow explore`
- stale or conflicting runtime/workgraph posture → `pulse:workflow use`

## Required inputs

Read only the artifacts needed to prove readiness for the active story/slice. Validation proof must be current, observable, and tied to the active story/slice; stale prior-run evidence and uncited confidence notes do not satisfy readiness.

Minimum story inputs:

- `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/discovery.md`
- `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/solution-design.md`
- approved `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/plan.md`
- TASK/BUG README files returned by workgraph `content_path` for the current slice
- TASK/BUG verification paths returned by workgraph `verification_path`, when already present or required

Runtime and workgraph inputs:

- `.pulse/runtime/state.json`
- `.pulse/runtime/STATE.md`
- `node .pi/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json`
- `node .pi/skills/workflow/scripts/pulse.mjs workgraph list --repo-root <repo> --json` or a narrower workgraph read when supported
- `node .pi/skills/workflow/scripts/pulse.mjs workgraph doctor --repo-root <repo> --json`

Optional targeted inputs:

- relevant repo files named by `plan.md` or TASK/BUG README file scope
- relevant test/config files needed to prove commands exist and are runnable
- docs paths named by the docs impact section when docs changes are part of the current slice
- story `references/*.md` only when `discovery.md`, `solution-design.md`, or `plan.md` cite them for a validation-critical assumption

Do not hand-edit `.pulse/workgraph/items.jsonl`. Treat it as database-like storage behind `node .pi/skills/workflow/scripts/pulse.mjs workgraph`.

## Core contracts

### Gate 2 is required input

Validate consumes the approved task/current-work shape. It must not mark Gate 2 approved, invent a plan approval, or materialize approved TASK/BUG items that planning failed to create.

If `plan.md` is approval-ready but not explicitly approved, stop for `pulse:workflow plan`. If the plan is approved but its approved TASK/BUG items or edges were not materialized, route back to the post-approval materialization portion of `pulse:workflow plan`.

### Plan and design are immutable during validation

Validate may test, inspect, and locally clarify execution readiness. It must not change solution decisions, task decomposition, docs impact, dependency shape, or verification strategy.

Allowed local repairs are limited to obvious non-semantic defects in TASK/BUG README content or runtime mirror wording when the approved plan and workgraph metadata are already clear. Examples: fixing a missing returned `verification_path` copied from workgraph output, or tightening a vague evidence record path that is already specified by `plan.md`.

Route instead of repairing when a defect changes any of:

- solution decision or planning constraint → `pulse:workflow design`
- task scope, sequencing, dependency, docs impact, or validation plan → `pulse:workflow plan`
- discovery evidence or external proof basis → `pulse:workflow explore`
- active runtime/workgraph ownership → `pulse:workflow use`

### Workgraph via CLI only

Use `node .pi/skills/workflow/scripts/pulse.mjs workgraph ... --json` for workgraph reads and consistency checks. Do not treat generated views or raw JSONL as writable truth. Use [workgraph-model.md](../shared/workgraph-model.md) for dependency, link, owner, reservation, and readiness semantics.

Validate may inspect workgraph output to confirm:

- active story and TASK/BUG IDs
- parent/child relationships
- blocking dependencies and non-blocking links
- `content_path` and `verification_path`
- status/readiness posture
- doctor output

Validate must not create speculative items, add unapproved edges, or close work.

### Probes are feasibility proof, not implementation

A probe/spike exists only to answer a readiness-blocking yes/no question. It must not implement the feature or silently alter the approved plan.

If a probe proves the plan needs different work, route back to `plan` or `design` with the finding.

## Phase model

### Phase 0 — Orientation and Gate 2 proof

Use the orientation template in [runtime-appendix.md](runtime-appendix.md#A-orientation-and-gate-2-proof-template), then confirm and present:

- active mode from `plan.md`
- active epic/story and selected current slice
- approved `plan.md` path and Gate 2 approval source
- referenced `solution-design.md` decision IDs
- materialized TASK/BUG IDs for the current slice
- runtime mirror sync status: `in-sync`, `out-of-sync`, or `missing`
- workgraph doctor status
- goal of the current slice

Hard stop when:

- `plan.md` is missing or not approved
- Gate 2 approval is not explicit
- approved TASK/BUG materialization required by the plan is incomplete
- runtime mirrors conflict with story artifacts or workgraph metadata
- workgraph doctor reports unresolved issues outside a local obvious repair

### Phase 1 — Runtime and workgraph consistency gate

Use the runtime/workgraph consistency template in [runtime-appendix.md](runtime-appendix.md#B-runtimeworkgraph-consistency-gate), then verify the active story/slice can be trusted:

- runtime active epic/story/item IDs match workgraph output
- TASK/BUG items are children of the active story
- dependencies only reference existing approved items or intentional existing blockers
- links are non-blocking traceability, not hidden dependencies
- `content_path` and `verification_path` stay under the owning story/work content area
- no active reservation or handoff conflict blocks validation
- generated readiness posture does not contradict Gate 2 state

If this fails because metadata is stale or conflicted, route to `pulse:workflow use` or the specific workgraph repair owner. If the approved item set is wrong, route to `pulse:workflow plan`.

### Phase 2 — Reality gate

Use the reality gate report shape in [runtime-appendix.md](runtime-appendix.md#C-reality-gate-template), then test whether the approved slice still fits real repository conditions:

- mode still matches size, risk, and current constraints
- planned files/modules/commands still exist or the plan accounts for creating them
- assumptions remain valid
- dependencies and boundary conditions still hold
- no safer smaller execution slice is being ignored without plan rationale
- verification commands and evidence paths are practically available

If the reality failure is task decomposition, route to `pulse:workflow plan`. If the approved solution is wrong or incomplete, route to `pulse:workflow design`. If evidence is missing, route to `pulse:workflow explore` or run a bounded probe when the question is narrow and validation-owned.

### Phase 3 — Feasibility matrix and probes

Use the feasibility matrix and probe protocol in [runtime-appendix.md](runtime-appendix.md#D-feasibility-matrix) and [runtime-appendix.md](runtime-appendix.md#G-probe--spike-protocol), then build an assumption-by-assumption matrix for the current slice:

- assumption statement
- risk level
- evidence required
- probe method or source
- pass/fail threshold
- consequence if disproven
- result

Required when:

- mode is `standard_feature` and meaningful assumptions remain
- mode is `high_risk_feature`
- any risk flag could invalidate execution safety
- verification depends on provider/runtime/tool behavior not already proven by repo evidence

High-impact, unproven assumptions require decisive probes with yes/no outcomes. Fuzzy confidence notes do not satisfy readiness.

Probe timebox policy:

- define one yes/no question per probe
- set a bounded attempt window before starting
- if inconclusive, stop and offer explicit options: extend, replan, constrain scope, or route to design/explore
- never classify inconclusive as ready

### Phase 4 — TASK/BUG contract gate

Use the TASK/BUG contract checklist in [runtime-appendix.md](runtime-appendix.md#E-taskbug-contract-checklist). For each current-slice TASK/BUG, verify contract quality from workgraph output and the item README:

- parent story is correct
- source plan and decision refs map back to `plan.md` and `solution-design.md`
- scope is bounded with explicit in-scope and out-of-scope sections
- explicit file scope is tight and justified
- dependencies match approved `plan.md` dependency rows and workgraph edges
- non-blocking links are not required for readiness
- testing mode is coherent with risk and plan validation strategy
- verification commands are concrete and runnable or intentionally deferred to execution with a clear reason
- verification evidence path is explicit and under the owning story
- docs impact expectations match `plan.md`
- risks/caveats are explicit, even when `None.`

If defects are local and obvious, repair once. If defects imply a task contract change, route to `pulse:workflow plan`. If they imply a solution change, route to `pulse:workflow design`.

### Phase 5 — Structural coherence pass

Use the structural checker contract in [runtime-appendix.md](runtime-appendix.md#F-structural-checker-contract), then validate end-to-end consistency across:

1. mode-plan coherence
2. current-slice coverage and ordering
3. locked decision coverage
4. dependency graph sanity
5. scope isolation and file-boundary credibility
6. context budget fit for the recommended execution mode
7. verification completeness and evidence credibility
8. docs impact and product/runtime contract consistency
9. integration and exit-state observability

Maximum correction loops: 3. After the third unresolved failure, stop and escalate with exact unresolved dimensions and reroute target.

### Phase 6 — Readiness decision

Use the readiness decision template in [runtime-appendix.md](runtime-appendix.md#H-readiness-decision-template), then return exactly one readiness decision:

- `ready`
- `ready-with-constraints`
- `not-ready`

Decision output must include:

- concrete rationale
- evidence summary
- blockers, if any
- constraints, if any
- missing-proof list, if any
- exact reroute target and required repairs for `not-ready`
- recommended execution mode: `swarm` or `single-worker`

Use `ready-with-constraints` only when execution is safe if the listed constraints are honored. Do not use it to bypass unresolved high-impact assumptions.

Execution mode guidance:

- recommend `swarm` only when multiple validated TASK/BUG items can proceed safely in parallel with clear dependency/reservation boundaries
- recommend `single-worker` when the slice is small, highly coupled, sequential, or coordination overhead would exceed benefit

### Phase 7 — Gate 3 approval hard stop

Execution cannot proceed without explicit user approval. Use the final approval prompt and runtime approval record in [runtime-appendix.md](runtime-appendix.md#I-final-approval-prompt-gate-3) and [runtime-appendix.md](runtime-appendix.md#J-runtime-approval-record).

Present the Gate 3 approval request with:

- readiness decision
- active story and current slice
- evidence and probes summary
- structural check result
- unresolved constraints or concerns
- recommended next command:
  - `pulse:workflow swarm` for validated parallel execution
  - `pulse:workflow execute` for validated single-worker execution

On approval:

- record Gate 3 approved state in `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json`
- set `gate: Gate 3` or the equivalent runtime gate field
- set `gate_status: approved`
- set `recommended_next_command` to `pulse:workflow swarm` or `pulse:workflow execute`
- set `next_action: manual_invoke` by default
- set `next_action: continue_now` only when the user explicitly approves continuing now

On rejection:

- capture rejection reason category
- route to the exact upstream owner: `plan`, `design`, `explore`, or `use`

Ambiguous approval language is not enough to start execution. Default continuation is manual.

## Gate posture

`pulse:workflow validate` enforces Gate 3.

Gate 3 approves execution readiness for the current slice only. It does not approve merge/ship quality, future slices, unplanned tasks, or implementation changes outside the approved scope.

## Role boundaries

Validate owns:

- Gate 2 and materialization proof
- runtime/workgraph consistency checks for current readiness
- feasibility truth testing
- assumption/probe matrix
- TASK/BUG contract quality checks
- execution-readiness decision
- Gate 3 approval request and runtime approval recording after explicit approval

Validate does not own:

- new-work admission
- direction setting
- discovery research beyond narrow readiness probes
- solution design changes
- task decomposition changes
- workgraph item creation/materialization
- implementation
- final product quality signoff
- merge or ship approval

## Pause/resume posture

If paused near context limits:

- write a validating-owned handoff snapshot under `.pulse/runtime/handoffs/`
- include completed phase, active story/slice, TASK/BUG IDs checked, probes run, open blockers, and next probe/action
- resume from Phase 0 orientation, then continue at the next incomplete phase

## Red flags

Stop if you catch yourself:

- validating without explicit Gate 2 approval
- treating `ready` as execution approval
- starting implementation or broad refactor while probing
- treating runtime mirrors as truth when artifacts or workgraph disagree
- approving with unresolved high-impact assumptions
- running structural checks before reality/feasibility clarity
- creating or editing workgraph items instead of routing to plan
- changing `solution-design.md` or `plan.md` during validation
- using vague `not-ready` language without an actionable repair path
- recommending `swarm` without parallel-safe validated item boundaries

## Exit contract

Successful exit requires:

- explicit readiness decision: `ready`, `ready-with-constraints`, or `not-ready`
- evidence summary and missing-proof list
- exact blocker/reroute path when not ready
- explicit execution mode recommendation
- Gate 3 approval outcome recorded when approved
- precise next command recommendation:
  - `pulse:workflow swarm`
  - `pulse:workflow execute`
  - `pulse:workflow plan`
  - `pulse:workflow design`
  - `pulse:workflow explore`
  - `pulse:workflow use`

