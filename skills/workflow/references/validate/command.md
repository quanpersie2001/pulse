# `pulse:workflow validate`

Operational feasibility and execution-approval manual for proving current work is real-world executable before any implementation begins.

This phase enforces Gate 3 discipline and prevents starting work on assumptions.

## Mission

Determine whether approved current work is executable under real repository constraints, return a precise readiness decision, and block execution until explicit approval.

## Entry criteria

Run `pulse:workflow validate` when:

- Gate 2 task plan is explicitly approved
- current-work contract exists
- execution has not started for this slice

Do not run when:

- task plan approval is pending or ambiguous
- planning artifacts and runtime mirrors disagree on active slice
- onboarding/readiness is stale or blocked

## Required inputs

- approved story-scoped design/planning artifacts under `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/` (`solution-design.md`, `PLAN.md`)
- canonical discovery/design artifacts in that same story directory (`discovery.md`, `solution-design.md`)
- approved task/current-work plan artifact in that same story directory, typically `PLAN.md` plus any current-work contract
- current-work artifacts (plan-dependent) in that same story directory
- `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json`
- existing current-slice workgraph items if already created (`.pulse/workgraph/items.jsonl`)

## Phase model (mandatory order)

### Phase 0 — Orientation (every run, including resumes)

Confirm and present:

- active mode
- approved task plan artifact and status
- current slice objective
- mirror sync status (in-sync / out-of-sync / missing)

Hard stop: if task plan is not approved or mirrors conflict with artifact truth, route back to planning/state sync.

### Phase 1 — Reality gate (fail fast)

Test whether planned work still fits current repo conditions:

- mode still appropriate
- assumptions still valid
- dependency and boundary conditions still true
- no safer smaller path being ignored without reason

If reality fails because task decomposition is wrong, route to `pulse:workflow plan`. If the approved solution is wrong or incomplete, route to `pulse:workflow design`.

### Phase 2 — Feasibility matrix

Build assumption-by-assumption matrix:

- assumption statement
- evidence required
- probe method
- pass/fail threshold
- consequence if disproven

High-impact, unproven assumptions require decisive probes (YES/NO outcomes), not fuzzy confidence notes.

Timebox policy for probes:

- bounded attempt window
- if inconclusive, escalate with explicit options (extend, replan, or constrain)
- never silently continue

### Phase 3 — Work-item schema gate (if items exist)

For each current-slice execution item, verify required contract quality:

- dependency correctness
- file/module scope boundedness
- verification commands are concrete
- evidence path is explicit
- testing mode is coherent with risk
- decision references map back to `solution-design.md`

If schema defects are local and obvious, repair.
If defects imply task contract change, route to planning. If they imply solution change, route to design.

### Phase 4 — Structural coherence pass

Validate end-to-end consistency across these dimensions:

1. mode-plan coherence
2. current-slice coverage and ordering
3. dependency graph sanity
4. scope isolation
5. verification completeness
6. integration and exit-state credibility

Iteration cap: max 3 correction loops. If still failing, escalate and stop.

### Phase 5 — Readiness decision

Return one of:

- `ready`
- `ready-with-constraints`
- `not-ready`

Decision must include:

- concrete rationale
- blockers (if any)
- exact reroute target and required repairs
- recommended execution mode: `swarm` or `single-worker`

### Phase 6 — Gate 3 approval hard stop

Execution cannot proceed without explicit user approval.

On approval:

- record Gate 3 approved state in `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json`
- recommend next command by mode:
  - `pulse:workflow swarm` for parallel-safe execution
  - `pulse:workflow execute` for single-worker execution
- default `next_action`: manual invoke unless user requests continue-now

On rejection:

- capture rejection reason category
- route to exact upstream artifact owner

## Gate posture

`pulse:workflow validate` enforces Gate 3.

Gate 3 is a strict human approval checkpoint; no implied approvals from confidence language, prior momentum, or “looks ready” wording.

## Role boundaries

Validate owns:

- feasibility truth testing
- execution safety decision
- reroute precision

Validate does not own:

- implementation
- final product quality signoff
- changing approved plan without planning loop or approved solution design without design loop

## Pause/resume posture

If paused near context limits:

- write a validating-owned handoff snapshot under `.pulse/runtime/handoffs/`
- include completed phase, open blockers, and next probe/action
- resume from orientation, then continue at next incomplete phase

## Red flags

- validating with unapproved task plan
- treating mirrors as truth when artifacts disagree
- approving with unresolved high-impact assumptions
- running structural checks before feasibility clarity
- allowing execution before explicit Gate 3 approval
- vague `not-ready` without actionable repair path

## Exit contract

Successful exit requires:

- explicit readiness decision
- explicit mode recommendation
- Gate 3 approval outcome recorded
- precise next command recommendation (`pulse:workflow swarm` or `pulse:workflow execute`, `pulse:workflow plan` for task repairs, or `pulse:workflow design` for solution repairs)

## References

- `runtime-appendix.md` — orientation, gate templates, schema checklist, and Gate 3 prompt
