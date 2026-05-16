# Pulse History Lifecycle Summary Contract

`history/<feature>/lifecycle-summary.md` is the durable audit summary for one feature.

It is not a resume artifact and it is not a replay log of transient runtime state.

## Purpose

Use this file to preserve the durable lifecycle of a feature at the level that matters for audit and onboarding:

- which approved artifacts define the feature
- which major gates were crossed
- which canonical verification evidence matters for audit
- which learnings/corrections/ratchet entries matter durably
- which follow-up debt remains after closeout

## What belongs here

- links to `CONTEXT.md`, `approach.md`, and `phase-plan.md`
- gate decisions and notable pivots that affected the durable outcome
- canonical verification paths under `history/<feature>/verification/`
- durable memory outputs promoted from compounding
- unresolved follow-up work worth carrying forward
- enough durable artifacts that a reader can reconstruct the feature-level delivery lifecycle from `history/<feature>/` alone for audit/onboarding purposes

## What does NOT belong here

- live `.pulse/runtime/state.json` mirrors
- active handoff internals
- transient runtime snapshots
- every checkpoint ever taken
- enough detail to resume work without the live control plane

## Suggested format

```markdown
# Lifecycle Summary

## Feature
- Feature: auth-refresh
- Final status: completed

## Approved artifacts
- Context: history/auth-refresh/CONTEXT.md
- Approach: history/auth-refresh/approach.md
- Phase plan: history/auth-refresh/phase-plan.md

## Gate outcomes
- GATE 1: CONTEXT approved
- GATE 2: phase plan approved
- GATE 3: execution approved
- GATE 4: review accepted after P1 fixes

## Canonical verification
- history/auth-refresh/verification/final-review.md
- history/auth-refresh/verification/uat-summary.md

## Durable memory promotions
- .pulse/memory/learnings/20260416-auth-refresh.md
- .pulse/memory/corrections/20260416-auth-guard.md
- .pulse/memory/ratchet/20260416-auth-regression.md

## Follow-up debt
- Follow-up: split token refresh retry policy into a separate bead
```

## Rules

1. Keep it concise enough to scan quickly.
2. Prefer promoted facts over live state descriptions.
3. Update it during reviewing/closeout, not during active execution.
4. If details conflict with active runtime state, the runtime state still governs live resume behavior.
5. A healthy history plane should let a future reader reconstruct the durable feature lifecycle from `history/<feature>/` without consulting `.pulse/` for in-flight mirrors.
6. `lifecycle-summary.md` is the index for that reconstruction, not the only artifact carrying lifecycle signal.
