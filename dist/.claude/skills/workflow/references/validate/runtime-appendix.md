# `pulse:workflow validate` Runtime Appendix

Reusable templates, checklists, and approval prompts for v2 validation.

This appendix assumes validate is consuming an approved story-scoped `plan.md` and any approved TASK/BUG work items already materialized through `node .claude/skills/workflow/scripts/pulse.mjs workgraph`.

## A. Orientation and Gate 2 proof template

```text
VALIDATE ORIENTATION
Mode: <spike | small_change | standard_feature | high_risk_feature>
Active story: <story-id> — <story title>
Current slice: <TASK/BUG IDs or story-level slice>
Approved plan: works/.../plan.md
Gate 2 status: APPROVED | PENDING | REVISE_REQUIRED | MISSING
Gate 2 source: <plan.md section / runtime record / explicit approval note>
Solution design: works/.../solution-design.md
Decision refs in slice: D...
Materialized items: <T-... / B-... / none required by plan>
Workgraph doctor: PASS | FAIL | NOT RUN
STATE mirror: in sync | out of sync | missing
Goal of current slice:
- <one-line practical outcome>
```

Stop conditions:

- `plan.md` is missing or not explicitly approved
- Gate 2 approval fields are missing where required
- approved TASK/BUG materialization required by `plan.md` is incomplete
- runtime mirrors, `plan.md`, and workgraph metadata disagree about active story/slice
- workgraph doctor fails with unresolved current-slice issues

## B. Runtime/workgraph consistency gate

```text
RUNTIME + WORKGRAPH CONSISTENCY
Runtime active story: <story-id | none>
Runtime active item(s): <ids | none>
Plan story: <story-id>
Workgraph story parent: PASS | FAIL
Materialized item set: PASS | FAIL
Dependency edges: PASS | FAIL
Traceability links: PASS | FAIL
Content paths: PASS | FAIL
Verification paths: PASS | FAIL
Reservation/handoff conflicts: PASS | FAIL
Decision: proceed | route to use | route to plan | repair local mirror wording
Evidence: <status/workgraph/doctor output and artifact paths>
```

PASS only when:

- every current TASK/BUG is under the active story
- workgraph IDs match the approved plan mapping
- dependency edges implement approved blocking dependencies
- links are non-blocking traceability only
- paths stay under `works/` and the owning story boundary where applicable
- no active reservation or handoff conflict makes validation unsafe

## C. Reality gate template

```text
REALITY GATE REPORT
Mode: <mode>
Current slice: <one sentence>
MODE FIT: PASS|FAIL
REPO FIT: PASS|FAIL
ASSUMPTIONS: PASS|FAIL
SMALLER PATH: PASS|FAIL
PROOF SURFACE: PASS|FAIL
VERIFICATION SURFACE: PASS|FAIL
Decision: proceed | revise plan | return to design | return to explore | run probe first
Evidence: <file/command/runtime evidence>
```

Fail if the plan assumes nonexistent code, unsupported commands, stale package/runtime behavior, missing credentials, unreachable services, hidden architecture work, missing verification surfaces, or too much ceremony for the approved slice.

## D. Feasibility matrix

Required for `standard_feature` when meaningful assumptions remain and always for `high_risk_feature`.

```text
FEASIBILITY MATRIX
Assumption | Risk | Proof Required | Probe / Source | Pass Threshold | Evidence | Result | Consequence If False
```

Accepted evidence:

- current implementation facts
- file/API/type inspection
- command output
- build/typecheck/test result
- official doc/version proof
- runtime/API probe
- bounded spike findings under `.spikes/<story-id>/<probe-id>/FINDINGS.md`
- story references cited by `discovery.md`, `solution-design.md`, or `plan.md`

Invalid evidence:

- “this should work”
- “likely”
- “expected”
- uncited model knowledge
- stale prior-run evidence not revalidated for the current slice

Feasibility results:

```text
READY
READY WITH CONSTRAINTS
NOT READY - RUN PROBE
NOT READY - RETURN TO PLAN
NOT READY - RETURN TO DESIGN
NOT READY - RETURN TO EXPLORE
```

READY is feasibility only. It is not Gate 3 approval and must not start execution.

## E. TASK/BUG contract checklist

For each current-slice TASK/BUG, verify both workgraph metadata and the item README.

Required contract fields:

- parent story ID and title
- source `plan.md` row or approved work mapping
- `solution-design.md` decision refs
- learning refs, even when empty
- objective
- in-scope and out-of-scope sections
- expected touched surfaces
- explicit file scope, including an explicit empty list when appropriate
- docs impact entries for required docs surfaces when the item touches docs/product/workflow contract
- implementation notes inherited from the approved plan, with no new solution decisions
- blocking dependencies and non-blocking links
- testing mode: `standard` or `tdd-required`
- explicit red/green commands when `testing_mode: tdd-required`
- verification commands with expected outcomes
- verification evidence path or concrete record
- caveats/risks, explicitly `None.` when none remain

Additional checks:

- dependency rows match workgraph `dep` edges
- link rows do not hide required execution order
- file scope is tight enough for safe execution/reservation
- verification command exists or has a plan-approved reason it can only run during execution
- verification evidence path stays under the owning story, normally `works/.../verification/<item-id>.md`
- decision refs exist in `solution-design.md`
- testing mode matches plan risk and validation strategy
- HIGH-risk items have meaningful learning refs when prior recall or project memory is relevant

Local repair is allowed only for obvious non-semantic omissions. Route to `pulse:workflow plan` for task shape, dependency, file scope, docs impact, or validation-plan changes. Route to `pulse:workflow design` for decision changes.

## F. Structural checker contract

Input set:

- approved story `plan.md`
- story `discovery.md`
- story `solution-design.md`
- current-slice TASK/BUG workgraph output
- current-slice TASK/BUG README files from `content_path`
- verification paths and existing verification artifacts when present
- targeted repo files/commands named by the current slice

PASS only when all dimensions pass:

1. mode/plan coherence
2. current-slice coverage and ordering
3. locked decision coverage
4. dependency correctness
5. file scope isolation and reservation credibility
6. context budget fit for recommended execution mode
7. verification completeness
8. docs impact consistency
9. integration/exit-state/risk coherence

Iteration policy:

- maximum 3 correction loops
- repair only local validate-owned defects
- fail after the third unresolved iteration and escalate with exact reroute

## G. Probe / spike protocol

Use a probe only for validation-owned feasibility proof.

Rules:

- one probe = one yes/no question
- define pass/fail threshold before probing
- timebox the attempt
- avoid feature implementation while probing
- write findings when the probe creates durable evidence:

```text
.spikes/<story-id>/<probe-id>/FINDINGS.md
```

Findings must close with:

```text
Result: YES | NO | INCONCLUSIVE
Evidence:
- ...
Impact:
- continue validation | route to plan | route to design | route to explore
```

If inconclusive:

- present current findings
- offer explicit options: extend, replan, constrain scope, or route upstream
- never classify inconclusive as YES

Routing:

- YES → ensure constraints already exist in `plan.md` or TASK/BUG contracts; route to `plan` if planning-owned artifacts must change
- NO → stop and route to `plan`, `design`, or `explore` depending on what failed
- INCONCLUSIVE → stop unless the user explicitly approves extension or scope constraint

## H. Readiness decision template

```text
VALIDATION READINESS DECISION
Decision: ready | ready-with-constraints | not-ready
Active story: <story-id>
Current slice: <TASK/BUG IDs or story-level slice>
Recommended execution mode: swarm | single-worker

Rationale:
- ...

Evidence summary:
- Runtime/workgraph: ...
- Reality gate: ...
- Feasibility: ...
- TASK/BUG contracts: ...
- Structural coherence: ...

Constraints:
- <None. | list constraints required for safe execution>

Missing proof:
- <None. | list exact missing evidence>

Blockers / reroute:
- <None. | pulse:workflow plan/design/explore/use with required repairs>
```

Use `ready-with-constraints` only when all high-impact assumptions are proven and remaining constraints are operational execution constraints, not unresolved design or planning questions.

## I. Final approval prompt (Gate 3)

```text
VALIDATION COMPLETE — APPROVAL REQUIRED BEFORE EXECUTION

Mode:
- Mode: <mode>
- Active story: <story-id> — <story title>
- Current slice: <TASK/BUG IDs or story-level slice>
- Approved plan: works/.../plan.md

Readiness:
- Decision: READY | READY WITH CONSTRAINTS
- Recommended execution mode: swarm | single-worker
- Recommended next command: pulse:workflow swarm | pulse:workflow execute

Reality + Feasibility:
- Runtime/workgraph consistency: PASS
- Reality gate: PASS
- Feasibility: READY | READY WITH CONSTRAINTS
- Probes: <none | passed | concerns>

Structure:
- TASK/BUG contract gate: PASS | not needed
- Structural checks: PASS (after <N> iterations)
- Verification evidence paths: PASS

Execution readiness:
- Entry state understood: YES
- Exit state observable: YES
- Integration readiness: YES
- Demo/proof credible: YES

Unresolved concerns / constraints:
- <none | list>
```

Approval options:

- Approve only
- Approve and continue now
- Revise plan
- Return to design
- Return to explore

Hard stop:

- no execution starts until explicit approval is captured
- default approval only updates runtime state with Gate 3 approval and `next_action: manual_invoke`
- execution starts only when the user explicitly chooses `Approve and continue now` or later manually invokes the recommended next command

## J. Runtime approval record

When Gate 3 is explicitly approved, update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md` together.

Minimum machine-readable posture should express:

```json
{
  "active_command": "validate",
  "gate": "Gate 3",
  "gate_status": "approved",
  "recommended_next_command": "pulse:workflow execute",
  "next_action": "manual_invoke"
}
```

Use `pulse:workflow swarm` as `recommended_next_command` when the validated slice is parallel-safe and swarm is the recommended mode.

Use `next_action: continue_now` only after explicit approval to continue immediately.
