# `/pulse validate` Runtime Appendix

Reusable templates, checklists, and approval prompts for validating.

## A. Orientation template

```text
Validating mode: <direct_task | spike | small_change | standard_feature | high_risk_feature>
Approved shape artifact: <work-shape.md | phase-plan.md | epic-map.md>
Current work: <direct work item | spike question | current story | current phase>
Approval status: APPROVED | PENDING | REVISE_REQUIRED
Approval source: works/.../<shape-artifact>
STATE mirror: in sync | out of sync | missing
Goal of current work:
- <one-line practical outcome>
```

Stop conditions:
- approved shape artifact is not `APPROVED`
- approval fields are missing where required
- shape artifact and `.pulse/runtime/STATE.md` disagree on approval/current work

## B. Reality gate template

```text
REALITY GATE REPORT
Mode: <mode>
Current work: <one sentence>
MODE FIT: PASS|FAIL
REPO FIT: PASS|FAIL
ASSUMPTIONS: PASS|FAIL
SMALLER PATH: PASS|FAIL
PROOF SURFACE: PASS|FAIL
Decision: proceed | revise planning | run spike first | collapse mode
Evidence: <file/command/runtime evidence>
```

Fail if the plan assumes nonexistent code, unsupported commands, stale versions, missing credentials, unreachable services, hidden architecture work, or too much ceremony.

## C. Feasibility matrix

Required for `standard_feature` when assumptions remain and always for `high_risk_feature`.

```text
FEASIBILITY MATRIX
Part / Assumption | Risk | Proof Required | Evidence | Result
```

Accepted evidence: implementation facts, file/API/type inspection, command output, build/typecheck/test result, official doc/version proof, runtime/API probe, or `.spikes/.../FINDINGS.md`.

Fail if evidence is only “this should work”, “likely”, “expected”, or model knowledge.

Decisions:

```text
READY
READY WITH CONSTRAINTS
NOT READY - RUN SPIKE
NOT READY - RETURN TO PLANNING
```

If feasibility is READY/READY WITH CONSTRAINTS and current-work items are required but absent:
- route to planning to create only validated current-work items
- resume validating at schema gate before Gate 3 approval

## D. Schema gate checklist

Every current-work item must include:
- `dependencies`
- `files`
- `verify`
- `verification_evidence`
- `testing_mode`
- `decision_refs`
- `learning_refs`

Additional checks:
- if `testing_mode: tdd-required`, include explicit red/green `tdd_steps`
- `verify` is executable proof criteria, not vague prose
- `verification_evidence` points to explicit artifact path or concrete record
- `files` scope is tight and justifiable
- for HIGH-risk items, `learning_refs` is populated when relevant recall exists

## E. Structural checker contract

Input set:
- current work items from `.pulse/workgraph/items.jsonl`
- story-scoped artifacts under `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/`: `CONTEXT.md`, `SPEC.md`, `DISCOVERY.md`, `APPROACH.md`
- approved shape artifact (`work-shape.md` | `phase-plan.md` | `epic-map.md`) and current-work contract docs in that same story directory

PASS only when all dimensions pass:
1. mode/shape coherence
2. current-work coverage and order
3. locked decision coverage
4. dependency correctness
5. file scope isolation
6. context budget fit
7. verification completeness
8. integration/exit-state/risk coherence

Iteration policy:
- maximum 3 iterations
- fail after third unresolved iteration and escalate

## F. Spike / probe protocol

- One spike = one yes/no question.
- Timebox isolated execution to 30 minutes.
- Write findings to `.spikes/<story-id>/<probe-id>/FINDINGS.md`.
- Close with definitive `YES` or `NO` only.

If no definitive answer at 30 minutes:
- present current findings
- offer: +15m extension (explicit approval), replan, or mitigation plan
- never classify inconclusive as YES

Routing:
- YES -> ensure constraints are reflected in affected current-work artifacts/items; route to planning when planning-owned artifacts must be updated
- NO -> stop, update planning artifacts, return to planning, re-run validating

## G. Final approval prompt (Gate 3)

```text
VALIDATION COMPLETE — APPROVAL REQUIRED BEFORE EXECUTION

Mode:
- Mode: <mode>
- Shape: <shape artifact>
- Current work: <story/work item>

Reality + Feasibility:
- Reality gate: PASS
- Feasibility: READY | READY WITH CONSTRAINTS
- Spikes: <none | passed | concerns>

Structure:
- Structural checks: PASS (after <N> iterations)
- Work-item schema: PASS | not needed

Execution readiness:
- Entry state understood: YES
- Exit state observable: YES
- Integration readiness: YES
- Demo/proof credible: YES

Unresolved concerns:
- <none | list>
```

Approval options:
- Approve only
- Approve and continue now
- Revise plan

Hard stop:
- no execution starts until explicit approval is captured
- default approval only updates runtime state with `gate_status: approved`, `next_skill_recommended`, and `next_action: manual_invoke`
- execution starts only when the user explicitly chooses `Approve and continue now` or later manually invokes the recommended next command
