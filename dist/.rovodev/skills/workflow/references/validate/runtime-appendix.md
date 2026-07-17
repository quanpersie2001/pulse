# `pulse:workflow validate` Runtime Appendix

Reusable templates, checklists, and reference material for validation.

This appendix assumes validate is consuming an approved story-scoped `plan.md` and any approved TASK/BUG work items already materialized through `node .rovodev/skills/workflow/scripts/pulse.mjs workgraph`.

## A. Orientation and Gate 2 proof

Confirm and present these fields:

- Mode: `spike | small_change | standard_feature | high_risk_feature`
- Active story: `<story-id> — <story title>`
- Current slice: `<TASK/BUG IDs or story-level slice>`
- Approved plan: `works/.../plan.md`
- Gate 2 status: `APPROVED | PENDING | REVISE_REQUIRED | MISSING`
- Gate 2 source: `<plan.md section / runtime record / explicit approval note>`
- Solution design: `works/.../solution-design.md`
- Decision refs in slice: `D...`
- Materialized items: `<T-... / B-... / none required by plan>`
- Workgraph doctor: `PASS | FAIL | NOT RUN`
- STATE mirror: `in sync | out of sync | missing`
- Goal of current slice: `<one-line practical outcome>`

Stop conditions:

- `plan.md` is missing or not explicitly approved
- Gate 2 approval fields are missing where required
- approved TASK/BUG materialization required by `plan.md` is incomplete
- runtime mirrors, `plan.md`, and workgraph metadata disagree about active story/slice
- workgraph doctor fails with unresolved current-slice issues

## B. Runtime/workgraph consistency gate

Verify these fields from runtime and workgraph output:

- Runtime active story: `<story-id | none>`
- Runtime active item(s): `<ids | none>`
- Plan story: `<story-id>`
- Workgraph story parent: `PASS | FAIL`
- Materialized item set: `PASS | FAIL`
- Dependency edges: `PASS | FAIL`
- Traceability links: `PASS | FAIL`
- Content paths: `PASS | FAIL`
- Verification paths: `PASS | FAIL`
- Reservation/handoff conflicts: `PASS | FAIL`
- Decision: `proceed | route to use | route to plan | repair local mirror wording`

PASS only when:

- every current TASK/BUG is under the active story
- workgraph IDs match the approved plan mapping
- dependency edges implement approved blocking dependencies
- links are non-blocking traceability only
- paths stay under `works/` and the owning story boundary where applicable
- no active reservation or handoff conflict makes validation unsafe

## C. Reality gate

Check these dimensions:

- MODE FIT: `PASS|FAIL` — mode still matches size, risk, and constraints
- REPO FIT: `PASS|FAIL` — planned files/modules/commands exist or plan accounts for creating them
- ASSUMPTIONS: `PASS|FAIL` — assumptions remain valid
- SMALLER PATH: `PASS|FAIL` — no safer slice being ignored without rationale
- PROOF SURFACE: `PASS|FAIL` — evidence paths practically available
- VERIFICATION SURFACE: `PASS|FAIL` — verification commands runnable

Decision: `proceed | revise plan | return to design | return to explore | run probe first`

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

- "this should work"
- "likely"
- "expected"
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

## F. Probe / spike protocol

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
