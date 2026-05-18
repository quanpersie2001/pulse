# `pulse:workflow review`

Final quality-gate manual for assessing completed current-slice work before release/merge closeout.

This phase is independent verification, not trust-based confirmation of execution claims.

## Mission

Determine whether delivered scope is correct, safe, and complete against approved boundaries and evidence standards, then enforce Gate 4.

## Entry criteria

Run `pulse:workflow review` when:

- execution for the approved current slice is complete
- verification evidence exists for all claimed completed work
- no upstream gate ambiguity remains for active slice

Do not run while execution is still in flight, boundaries are disputed, or onboarding/readiness is stale.

## Required inputs

Read before starting:

- active current-slice artifacts under `works/`
- `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md`
- lifecycle summary for the active story/slice when present
- completed change set and verification evidence artifacts
- minimal project context docs (README/architecture/ADR) when needed for correctness checks

## Runtime contract

`runtime-appendix.md` is canonical for:

- 4+1 review orchestration
- severity mapping and review-item creation rules
- Gate 4 hard-block behavior for P1
- evidence verification contract
- UAT/acceptance routing
- finishing checklist and closeout

## Minimum flow (mandatory)

1. Run specialist review pass (4+1 model).
2. Verify promised artifacts and evidence freshness.
3. Normalize findings, deduplicate overlap, assign owner + severity.
4. Convert accepted findings into explicit fix work items.
5. Enforce Gate 4: any P1 blocks approval.
6. Run UAT/acceptance checks unless explicitly skipped with compensating evidence.
7. Execute finishing checklist and update runtime state artifacts.
8. Recommend next command with manual default continuation.

## Phase details

### Phase 1 — Scope conformance audit

Confirm delivered changes stayed within approved current-slice boundaries and locked decisions.

If material mismatch exists, block and route to `pulse:workflow plan` or `pulse:workflow execute` as appropriate.

### Phase 2 — Specialist quality pass

Assess behavior correctness, regressions, boundary integrity, security posture, and contract adherence.

Reviewers do not silently fix and approve in one motion. Findings must remain explicit.

Use `review-agent-prompts.md` to run consistent specialist prompts.

### Phase 3 — Evidence integrity pass

For each closed work item, confirm evidence is:

- present
- fresh
- relevant
- reproducible enough for audit

Reject stale or non-specific evidence.

### Phase 4 — Finding normalization and severity

Deduplicate overlaps, assign ownership, and classify severity.

Severity contract:

- P1 = mandatory blocker
- P2/P3/P4 may be staged, never hidden

When accepted findings need remediation, create explicit fix work items using `review-item-template.md`.

### Phase 5 — Gate 4 enforcement

If any P1 exists:

- Gate 4 fails
- publish blocking item IDs and required remediation path
- do not recommend approval

If no P1 remains and evidence is sufficient, Gate 4 may pass.

### Phase 6 — Acceptance/UAT posture

Run acceptance checks unless skipped by explicit instruction.

If skipped, record compensating evidence and residual risk.

If acceptance fails, route back with targeted repair scope.

### Phase 7 — Closeout handoff

Publish:

- approve/block decision
- finding inventory by severity and owner
- exact next command recommendation:
  - `pulse:workflow execute` for fixes
  - `pulse:workflow plan` for scope/shape repair
  - `pulse:workflow compound` after clean pass

Default continuation is `manual_invoke` unless user asks continue-now.

Update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md` to reflect Gate 4 outcome.

## Pause/resume posture

If review pauses, write review-owned handoff, persist it under `.pulse/runtime/handoffs/`, and register in `.pulse/runtime/handoffs/manifest.json`.

Resume from next incomplete review phase, not memory-only recall.

## Red flags

- approving because execution said “done”
- accepting stale/partial evidence
- blurring P1/P2 boundaries
- mixing reviewer and implementer roles without explicit reroute
- recommending release with unresolved mandatory blockers

## References

- `runtime-appendix.md`
- `review-agent-prompts.md`
- `review-item-template.md`
