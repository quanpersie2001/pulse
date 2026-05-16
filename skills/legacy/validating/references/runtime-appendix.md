# Validating Runtime Appendix

This is the single runtime appendix for `pulse:validating`. It contains reusable prompts, checklists, and approval templates used by the hot path.

---

## A. Orientation Template

```text
Validating mode: <direct_task | spike | small_change | standard_feature | high_risk_feature>
Approved shape artifact: <work-shape.md | phase-plan.md | epic-map.md>
Current work: <direct work item | spike question | current story | current phase>
Approval status: APPROVED | PENDING | REVISE_REQUIRED
Approval source: history/<feature>/<shape-artifact>
STATE mirror: in sync | out of sync | missing
Goal of current work:
- <one-line practical outcome>
```

Stop conditions:
- approved shape artifact is not `APPROVED`
- approval fields are missing where required
- shape artifact and `.pulse/STATE.md` disagree on approval/current work

---

## B. Reality Gate Template

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

---

## C. Feasibility Matrix

Required for `standard_feature` when assumptions remain and always for `high_risk_feature`.

```text
FEASIBILITY MATRIX
Part / Assumption | Risk | Proof Required | Evidence | Result
```

Accepted evidence: existing implementation, file/API/type inspection, command output, build/typecheck/test result, official version/doc proof, runtime/API probe, or `.spikes/<feature>/` proof.

Fail if evidence is only “this should work”, “likely”, “expected”, or model knowledge.

Decisions:

```text
READY
READY WITH CONSTRAINTS
NOT READY - RUN SPIKE
NOT READY - RETURN TO PLANNING
```

READY is feasibility, not execution approval, until required current-work beads pass review.

If feasibility is READY/READY WITH CONSTRAINTS and beads are required for current work but absent:
- route to planning to create only validated current-work beads
- resume validating at schema gate and continue through bead review before Gate 3 approval

---

## D. Schema Gate Checklist

Every bead must include:
- `dependencies`
- `files`
- `verify`
- `verification_evidence`
- `testing_mode`
- `decision_refs`
- `learning_refs`

Additional checks:
- if `testing_mode: tdd-required`, bead must include explicit red/green `tdd_steps`
- `verify` must be executable proof criteria, not vague prose
- `verification_evidence` must point to explicit artifact path or concrete record
- `files` scope must be tight and justifiable
- for HIGH-risk beads, `learning_refs` must be populated when clearly relevant recall exists

---

## E. Plan-Checker Runtime Contract

Use when structure validation is needed after feasibility passes.

Input set:
- all `.beads/*.md`
- `history/<feature>/CONTEXT.md`
- `history/<feature>/discovery.md`
- `history/<feature>/approach.md`
- approved shape artifact (`work-shape.md` / `phase-plan.md` / `epic-map.md`)
- current-work artifacts (`current-story-pack.md` or `phase-<n>-contract.md` + `phase-<n>-story-map.md`)

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

---

## F. Spike / Probe Protocol

- One spike = one yes/no question.
- Timebox isolated execution to 30 minutes.
- Write findings to `.spikes/<feature>/<spike-id>/FINDINGS.md`.
- Close with definitive `YES` or `NO` only.

If no definitive answer at 30 minutes:
- present current findings
- offer: +15m extension (explicit approval), replan, or mitigation plan
- never classify inconclusive as YES

Routing:
- YES -> ensure constraints are reflected in affected current-work artifacts/beads; route to planning when planning-owned artifacts must be updated
- NO -> stop, update planning artifacts, return to planning, re-run validating

---

## G. Bead Fresh-Eyes Reviewer Contract

Review goals:
- detect CRITICAL execution blockers (assumed context, vague done criteria, broken verify, missing schema, missing TDD contract, scope overload, ambiguous path)
- detect MINOR ambiguity/judgment-call risks
- revise only when fix is local, clear, and keeps intended scope

Report skeleton:

```text
BEAD REVIEW REPORT
CRITICAL FLAGS (<N>)
MINOR FLAGS (<N>)
CLEAN BEADS (<N>)
REVISIONS MADE (<N>)
SUMMARY
```

Gate rule:
- all CRITICAL flags must be resolved before approval request

---

## H. Final Approval Prompt (Gate 3)

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

Structure + Beads:
- Structural checks: PASS (after <N> iterations)
- Bead review: <done | not needed>
- Fresh-eyes CRITICAL flags fixed: <N>

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
- Review beads
- Revise plan

Hard stop:
- no execution starts until explicit approval is captured
- default approval only updates runtime state with `gate_status: approved`, `next_skill_recommended`, and `next_action: manual_invoke`
- execution starts only when the user explicitly chooses `Approve and continue now` or later manually invokes the recommended next skill
