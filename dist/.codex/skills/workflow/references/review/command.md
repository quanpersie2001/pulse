# `pulse:workflow review`

Gate 4 quality-review manual for assessing a completed Gate 3 execution slice before merge, release, closeout, or compounding.

Review answers:

> Did the completed current slice satisfy the approved plan/design, produce trustworthy evidence, surface implementation gaps, and avoid blocking quality risks?

Review is independent verification. It does not trust execution claims, silently fix findings, reopen design choices, or approve merge/release by implication.

## Mission

Evaluate completed `TASK`/`BUG` work against approved story boundaries, item contracts, verification evidence, implementation gap logs, and the actual change set; classify findings; create explicit remediation items when needed; and enforce Gate 4.

P1 findings or missing required evidence block Gate 4 approval.

## Entry criteria

Run `pulse:workflow review` only when all are true:

- Gate 3-approved current-slice execution is complete
- no worker, reservation, or commit queue remains in flight for the slice
- every claimed completed `TASK`/`BUG` is `CLOSED` in the workgraph
- each completed item has fresh verification evidence at its `verification_path`
- any `implement-gap.md` created during execution is present and surfaced
- runtime state, workgraph metadata, and story artifacts agree on the active epic/story/slice

Do not run while execution is still active, item boundaries are disputed, Gate 3 is ambiguous, or onboarding/runtime posture is stale.

If entry fails, reroute precisely:

| Failure | Reroute |
| --- | --- |
| Runtime/session posture unclear or stale | `pulse:workflow use` |
| Execution still in flight or evidence incomplete | `pulse:workflow execute` or `pulse:workflow swarm` |
| Item contract or slice boundary mismatch | `pulse:workflow plan` |
| Approved solution/design mismatch discovered before review can proceed | `pulse:workflow design` |

## Command-local references

- [runtime-appendix.md](runtime-appendix.md) — concise review checklists for severity, evidence, implementation gaps, findings, Gate 4, UAT, closeout, and handoff
- [review-agent-prompts.md](review-agent-prompts.md) — focused specialist review prompts
- [review-item-template.md](review-item-template.md) — remediation item shape for accepted findings

## Core invariants

- Review evaluates; it does not implement fixes unless explicitly rerouted to execute.
- Approved `solution-design.md`, approved `plan.md`, item README contracts, verification evidence, and implementation gap logs are the review baseline.
- A closed workgraph item is not proof of correctness. Evidence and diff must still be checked.
- `implement-gap.md` entries must be evaluated, not ignored as notes.
- P1 findings block Gate 4 until fixed or explicitly acknowledged under the project’s gate policy.
- P2/P3/P4 findings may be deferred only when they are explicit, owned, and traceable.

## Phase flow

```text
Orient -> Scope & Evidence Audit -> Specialist Review -> Normalize Findings
  -> Create Fix Items -> Gate 4 Decision -> Acceptance/UAT -> Closeout Handoff
```

## Phase 1 — Orient and prove review readiness

Read in this order:

1. `AGENTS.md`
2. `node .codex/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json`
3. `.pulse/runtime/state.json`
4. `.pulse/runtime/STATE.md`
5. active story `plan.md`
6. active story `solution-design.md`
7. item README files for completed current-slice `TASK`/`BUG` items
8. each item `verification_path`
9. each item `implement-gap.md` when present
10. current git diff or committed range for the completed slice

Then confirm:

- active epic/story/slice pointers match workgraph output
- all current-slice executable items are closed or intentionally excluded from this review
- no active reservations or handoffs indicate in-flight execution
- verification evidence and implementation gap logs belong to this execution pass

If readiness cannot be proven, stop and reroute. Do not review from stale or guessed context.

## Phase 2 — Scope and evidence audit

Check delivered scope against:

- `solution-design.md` decision IDs
- approved `plan.md` scope, docs impact, and validation plan
- each item README contract
- actual changed files
- verification evidence
- `implement-gap.md` entries

Reject review readiness when required evidence is missing or stale.

If implementation gaps record an approved deviation, verify the approval is explicit and the implementation matches it. If a gap records an unapproved behavior, architecture, file-scope, verification, risk, or public-contract change, classify it as a finding and usually P1/P2 depending on impact.

## Phase 3 — Specialist review pass

Run focused review passes using [review-agent-prompts.md](review-agent-prompts.md):

1. behavior correctness
2. regression and boundary integrity
3. security and misuse risk
4. evidence and implementation-gap integrity
5. release-readiness synthesis

Agents/review passes should lead with findings and cite concrete evidence: file paths, line numbers, item IDs, verification artifacts, gap logs, or command outputs.

Do not ask reviewers to rewrite code. Findings must remain review artifacts until routed to execute/plan/design.

## Phase 4 — Normalize findings

Deduplicate overlap, assign severity, owner, affected item IDs, and reroute target.

Severity rules are in [runtime-appendix.md](runtime-appendix.md#severity-policy).

Each accepted finding must include:

- severity `P1|P2|P3|P4`
- affected item/story
- evidence path or file/line reference
- failure scenario or concrete risk
- smallest credible fix or repair path
- recommended reroute: `execute`, `plan`, `design`, `explore`, or `none`

## Phase 5 — Create explicit remediation items

When a finding requires implementation work, create or propose a workgraph `BUG` or `TASK` under the active story using `node .codex/skills/workflow/scripts/pulse.mjs workgraph create ... --json`, but only when the user/project policy permits review to materialize fix work.

Use [review-item-template.md](review-item-template.md) for the README/body shape.

Rules:

- P1 findings become blocking fix items unless immediately fixed through a user-approved execute reroute.
- P2/P3/P4 findings may become non-blocking follow-up items or linked traceability when deferrable.
- Do not attach unrelated future cleanup as current-slice blockers.
- Do not use `linked_items` when readiness or Gate 4 depends on the fix; use dependencies/blocking status where appropriate.
- Do not silently create fix items for design/plan changes; reroute to the owning command when the fix changes shape or solution.

## Phase 6 — Gate 4 decision

Return exactly one Gate 4 decision:

- `pass`
- `pass-with-follow-ups`
- `fail`

Gate 4 passes only when:

- no P1 remains
- required verification evidence is fresh and sufficient
- implementation gaps are reviewed and either accepted, fixed, or explicitly deferred
- acceptance/UAT posture is explicit

Gate 4 fails when any P1 remains, required evidence is missing, or acceptance/UAT fails.

Record the rationale, finding inventory, and next command.

## Phase 7 — Acceptance/UAT posture

Run acceptance checks when the slice has user-visible behavior, workflow-visible behavior, docs/contract changes, or explicit UAT criteria in the plan.

If UAT is skipped, record:

- who/what skipped it
- reason
- compensating evidence
- residual risk

UAT failure is a finding and normally routes to `pulse:workflow execute` with a targeted fix item.

## Phase 8 — Closeout handoff

Publish:

- Gate 4 decision
- reviewed story/slice and item IDs
- evidence summary
- implementation gap summary
- findings inventory by severity and owner
- fix item IDs when created
- exact next command recommendation:
  - `pulse:workflow execute` for implementation fixes
  - `pulse:workflow plan` for task/scope repair
  - `pulse:workflow design` for solution repair
  - `pulse:workflow compound` after clean pass or accepted follow-ups

Update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md` together when recording Gate 4 posture.

Default continuation is `manual_invoke` unless the user explicitly asks to continue now.

## Pause/resume posture

If review pauses, write a review-owned handoff under `.pulse/runtime/handoffs/` and register it in `.pulse/runtime/handoffs/manifest.json`.

Resume from Phase 1 orientation, then continue at the next incomplete phase. Do not resume review from memory-only recall.

## Red flags

Stop if you catch yourself:

- approving because execution said `[DONE]`
- accepting stale, missing, or non-specific evidence
- ignoring `implement-gap.md`
- marking an unapproved deviation as acceptable without an explicit decision
- blurring P1/P2 boundaries
- mixing reviewer and implementer roles without explicit reroute
- recommending `compound` with unresolved mandatory blockers
- creating unplanned fix work without user/project authorization
