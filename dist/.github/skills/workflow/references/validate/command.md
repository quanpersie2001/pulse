# `pulse:workflow validate`

Readiness command for proving an approved `plan.md` and its current-slice work items are executable before implementation begins.

`pulse:workflow validate` sits after plan approval and before execution.

Validate answers:

> Is this approved execution slice feasible, coherent, and safe to start now?

Validate enforces Gate 3. A `ready` decision is not execution approval until the user explicitly approves Gate 3.

## Mission

Produce a precise execution-readiness decision for the approved current slice, expose blockers with exact reroutes, recommend `swarm` or `execute`, and stop for explicit Gate 3 approval.

Validate must prove:

- Gate 2 approved `plan.md` exists and is the active planning artifact
- approved Ticket materialization required by the plan is complete
- daemon posture, story artifacts, and workgraph metadata point to the same active slice
- planned work still fits the current repository reality
- high-impact assumptions are proven or routed to a bounded probe/spike
- Ticket contracts contain enough file scope, dependency, verification, and evidence detail for execution

## Entry criteria

Run `pulse:workflow validate` when:

- `pulse:workflow plan` has completed Gate 2 approval
- story `plan.md` is approved and current
- approved Ticket work items have been materialized through `pulse work` when the plan requires them
- execution has not started for the selected slice
- runtime/workgraph posture can identify the active epic/story and current slice

Do not run when:

- Gate 2 approval is pending, ambiguous, rejected, or not recorded
- `plan.md`, daemon posture, and workgraph metadata disagree about active work
- approved Ticket materialization from `plan.md` is incomplete
- onboarding/readiness is stale or blocked
- the user is asking to implement, fix, review, or merge rather than prove readiness

If entry criteria fail, route precisely:

- missing or unapproved `plan.md` → `pulse:workflow plan`
- approved design gap discovered while validating → `pulse:workflow design`
- missing discovery/evidence needed to decide feasibility → `pulse:workflow explore`
- stale or conflicting runtime/workgraph posture → `pulse:workflow use`

## Inputs

Read only the artifacts needed to prove readiness for the active story/slice. Validation proof must be current, observable, and tied to the active story/slice; stale prior-run evidence and uncited confidence notes do not satisfy readiness.

### Minimum story inputs

- `works/<story-id>/discovery.md`
- `works/<story-id>/solution-design.md`
- approved `works/<story-id>/plan.md`
- Ticket README files under each returned `value.content_dir` for the current slice
- declared Ticket verification evidence paths, when already present or required

### Runtime and workgraph inputs

- daemon posture from `pulse daemon status`
- the owning work artifact
- `pulse daemon status`
- `pulse work list --repo-root <repo> --json` or a narrower workgraph read when supported
- `pulse graph validate --repo-root <repo> --json`

### Optional targeted inputs

- relevant repo files named by `plan.md` or Ticket README file scope
- relevant test/config files needed to prove commands exist and are runnable
- docs paths named by the docs impact section when docs changes are part of the current slice
- story `references/*.md` only when `discovery.md`, `solution-design.md`, or `plan.md` cite them for a validation-critical assumption

Do not hand-edit `.pulse/workgraph/nodes/`. Treat it as database-like storage behind supported Rust `pulse work` commands.

## Command-local references

- [runtime-appendix.md](runtime-appendix.md) — orientation fields, consistency gate, reality gate, feasibility matrix, Ticket contract checklist, and probe protocol

## Core contracts

### Gate 2 is required input

Validate must not mark Gate 2 approved, invent a plan approval, or materialize Ticket items that planning failed to create. See [Phase 0](#phase-0--orientation-and-gate-2-proof) for enforcement.

### Plan and design are immutable during validation

Validate may test, inspect, and locally clarify execution readiness. It must not change solution decisions, task decomposition, docs impact, dependency shape, or verification strategy. Allowed local repairs are limited to obvious non-semantic defects (e.g., tightening a vague evidence path already specified by `plan.md`). Route instead of repairing — see [Reroutes](#reroutes).

### Workgraph via CLI only

Use `pulse work ... --json` for reads and consistency checks. Do not treat graph projections or generated graph data as writable truth. Validate must not create speculative items, add unapproved edges, or close work. Use [workgraph-model.md](../shared/workgraph-model.md) for semantics.

### Probes are feasibility proof, not implementation

A probe exists only to answer a readiness-blocking yes/no question. It must not implement the feature or silently alter the approved plan. See [Phase 3](#phase-3--feasibility-matrix-and-probes) for probe protocol.

## Phase model

### Phase 0 — Orientation and Gate 2 proof

Use the orientation fields in [runtime-appendix.md](runtime-appendix.md#a-orientation-and-gate-2-proof), then confirm and present:

- active mode from `plan.md`
- active epic/story and selected current slice
- approved `plan.md` path and Gate 2 approval source
- referenced `solution-design.md` decision IDs
- materialized Ticket IDs for the current slice
- daemon status confirmation when runtime work is involved
- `pulse graph validate --repo-root <repo> --json` status
- goal of the current slice

Hard stop when:

- `plan.md` is missing or not approved
- Gate 2 approval is not explicit
- approved Ticket materialization required by the plan is incomplete
- daemon posture conflicts with story artifacts or workgraph metadata
- `pulse graph validate` reports unresolved issues outside a local obvious repair

### Phase 1 — Runtime and workgraph consistency gate

Use the runtime/workgraph consistency template in [runtime-appendix.md](runtime-appendix.md#b-runtimeworkgraph-consistency-gate), then verify the active story/slice can be trusted:

- runtime active epic/story/item IDs match workgraph output
- Ticket items are children of the active story
- dependencies only reference existing approved items or intentional existing blockers
- links are non-blocking traceability, not hidden dependencies
- `value.content_dir` and declared evidence paths stay under the owning story/work content area
- no active reservation or handoff conflict blocks validation
- generated readiness posture does not contradict Gate 2 state

If this fails because metadata is stale or conflicted, route to `pulse:workflow use` or the specific workgraph repair owner. If the approved item set is wrong, route to `pulse:workflow plan`.

### Phase 2 — Reality gate

Use the reality gate dimensions in [runtime-appendix.md](runtime-appendix.md#c-reality-gate), then test whether the approved slice still fits real repository conditions:

- mode still matches size, risk, and current constraints
- planned files/modules/commands still exist or the plan accounts for creating them
- assumptions remain valid
- dependencies and boundary conditions still hold
- no safer smaller execution slice is being ignored without plan rationale
- verification commands and evidence paths are practically available

If the reality failure is task decomposition, route to `pulse:workflow plan`. If the approved solution is wrong or incomplete, route to `pulse:workflow design`. If evidence is missing, route to `pulse:workflow explore` or run a bounded probe when the question is narrow and validation-owned.

### Phase 3 — Feasibility matrix and probes

Use the feasibility matrix and probe protocol in [runtime-appendix.md](runtime-appendix.md#d-feasibility-matrix) and [runtime-appendix.md](runtime-appendix.md#f-probe--spike-protocol), then build an assumption-by-assumption matrix for the current slice:

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

### Phase 4 — Ticket contract gate

Use the Ticket contract checklist in [runtime-appendix.md](runtime-appendix.md#e-taskbug-contract-checklist). For each current-slice Ticket, verify contract quality from workgraph output and the item README:

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

Validate end-to-end consistency across:

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

Return exactly one readiness decision:

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

- recommend `swarm` only when multiple validated Ticket items can proceed safely in parallel with clear dependency/reservation boundaries
- recommend `single-worker` when the slice is small, highly coupled, sequential, or coordination overhead would exceed benefit

### Phase 7 — Gate 3 approval hard stop

Execution cannot proceed without explicit user approval.

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

- record Gate 3 approved state in the work artifact and confirm live posture with `pulse daemon status`
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
- Ticket contract quality checks
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

- record a validating-owned handoff note in the owning work artifact
- include completed phase, active story/slice, Ticket IDs checked, probes run, open blockers, and next probe/action
- resume from Phase 0 orientation, then continue at the next incomplete phase

## Red flags

Stop if you catch yourself:

- validating without explicit Gate 2 approval
- treating `ready` as execution approval
- starting implementation or broad refactor while probing
- treating daemon posture as truth when artifacts or workgraph disagree
- approving with unresolved high-impact assumptions
- running structural checks before reality/feasibility clarity
- creating or editing workgraph items instead of routing to plan
- changing `solution-design.md` or `plan.md` during validation
- using vague `not-ready` language without an actionable repair path
- recommending `swarm` without parallel-safe validated item boundaries

## Reroutes

Each phase specifies its own routing. Summary:

| Target | When |
|--------|------|
| `pulse:workflow plan` | incomplete Ticket materialization; task scope/sequencing/dependency/docs/validation-plan change; reality gate fails on task decomposition |
| `pulse:workflow design` | solution decision wrong/incomplete/infeasible; reality gate fails on approved solution; probe proves plan needs different work |
| `pulse:workflow explore` | discovery evidence missing; external/provider/security research needed |
| `pulse:workflow use` | runtime/workgraph posture stale/blocked/conflicts; mirrors disagree with artifacts or workgraph |
