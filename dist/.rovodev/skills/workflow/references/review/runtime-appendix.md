# `pulse:workflow review` Runtime Appendix

Compact checklists and contracts for `pulse:workflow review`. The command file owns the review flow; this appendix should stay short.

## 4+1 review model

Run five focused passes:

1. behavior correctness
2. regression and boundary integrity
3. security and misuse risk
4. evidence and implementation-gap integrity
5. release-readiness synthesis

Use [review-agent-prompts.md](review-agent-prompts.md) for pass prompts.

## Severity policy

- **P1**: mandatory blocker; Gate 4 fails. Security breach, data loss, breaking change, unapproved behavior/public-contract deviation, missing required evidence, or production blocker.
- **P2**: serious reliability, architecture, performance, test, or maintainability issue; may proceed only with explicit owner plan.
- **P3**: bounded quality debt or follow-up with low immediate risk.
- **P4**: minor cleanup or polish.

When uncertain between P1 and P2, choose P1 if merge/release could expose users or corrupt the approved contract; otherwise choose P2 and state the uncertainty.

## Evidence integrity checklist

For each reviewed `TASK`/`BUG`, verify:

- workgraph item is closed for this execution pass
- evidence file exists at `verification_path`
- evidence lists commands, exit codes, and observed outputs
- evidence maps to the reviewed diff/commit range
- unresolved gaps are explicit
- `tdd-required` items include red/green evidence
- generated artifacts, screenshots, logs, or docs updates referenced by evidence exist when claimed

Closed item metadata is not evidence by itself.

## Implementation gap review

For each item, check whether `implement-gap.md` exists under the item directory.

If present, verify:

- every recorded decision/tradeoff is either within approved scope or has explicit approval
- deviations from `solution-design.md`, `plan.md`, item README, file scope, verification strategy, or public contract are not silently accepted
- follow-ups/reviewer notes are represented in findings or explicitly deferred

If absent, confirm the diff and evidence do not show unrecorded decisions, deviations, or tradeoffs that should have been logged.

## Finding normalization

Each accepted finding must include:

- severity `P1|P2|P3|P4`
- affected story/item IDs
- evidence path or file/line reference
- concrete failure scenario or risk
- smallest credible fix
- recommended reroute: `execute | plan | design | explore | none`

Use [review-item-template.md](review-item-template.md) when a remediation item is created or proposed.

## Gate 4 contract

Gate 4 passes only when:

- no P1 remains
- required evidence is fresh and sufficient
- implementation gaps are reviewed and resolved, accepted, or explicitly deferred
- acceptance/UAT posture is explicit

Gate 4 fails when any P1 exists, required evidence is missing, acceptance/UAT fails, or an unapproved mandatory deviation remains.

## UAT routing

- default: run acceptance checks for user-visible/workflow-visible/docs-contract changes
- skip only with explicit reason plus compensating evidence
- UAT failure routes to `pulse:workflow execute` with a targeted fix item unless it proves a plan/design issue

## Closeout outputs

Publish:

- Gate 4 decision: `pass | pass-with-follow-ups | fail`
- reviewed story/slice and item IDs
- evidence summary
- implementation gap summary
- finding inventory by severity/owner
- blocking fix item IDs when any
- next command recommendation

## Handoff for review

Use the shared envelope from [`../use/handoff-contract.md`](../use/handoff-contract.md). For review pauses, include the active review phase, reviewed item IDs, evidence paths, implementation gap paths, findings drafted so far, open UAT posture, and next review action. Do not duplicate the shared schema here.

## Red flags

Stop if you notice:

- approving because execution reported done
- accepting stale or partial evidence
- ignoring `implement-gap.md`
- marking UAT failure as pass
- P1 without fix or explicit gate handling
- P2/P3 hidden instead of tracked
- reviewer silently fixes code while reviewing
