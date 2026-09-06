# Agent Guidance

## Repository purpose

A minimal todo-list app. It exists as the real target repository the Pulse
harness dogfoods: every feature Pulse claims is exercised here first.

## Source map

- `src/todolist.mjs` owns todo domain logic (pure functions, no I/O).
- `src/cli.mjs` owns the command-line entry point and state file handling.
- `test/todolist.test.mjs` owns executable behavior examples (`node --test`).
- `docs/product/todolist.md` owns the user-visible behavior contract.
- `scripts/verify.mjs` is the deterministic verification entry point.
- `scripts/qa-run.mjs` is the QA runner role script (reads the run input
  contract, records a `qa_checkpoint` receipt, prints one final JSON line).

## Constraints

- **LRN-002 — Required-docs Tickets must record and reference the documentation receipt before handoff**: On Tickets with docs posture required, a handoff whose proofs do not reference a current documentation_validation receipt reads as a broken proof chain: the reviewer reworks it even when every check, QA case and doc validation actually passed. Hit on TK-004, TK-005 and TK-008; all three rework cycles were this single gap.
  - Do: Re-record the receipt whenever the doc content changes after the last validation
  - Do: Reference the returned rcpt id in the handoff with --evidence-receipt and in the --proof mappings
  - Do: Run pulse docs validate --record --actor <actor> after the behavior doc is final
  - Avoid: Recording the receipt but leaving it unreferenced in the handoff
  - Avoid: Treating a passed docs validate as equivalent to a bound receipt
  - Check: The handoff receipt's evidence_receipt_ids contains a documentation_validation receipt for DOC-TODOLIST-BEHAVIOR bound to the current doc content hash

- **LRN-001 — Freeze the target tree between handoff and close**: Editing any tracked file in the target tree after handoff stales the handoff dirty fence: verification is refused and close cannot pass until a fresh cycle.
  - Do: Leave the target worktree untouched between handoff and close
  - Do: Recover a stale proof chain with work release, then re-run worker, reviewer and close
  - Avoid: Editing tracked files, including tooling scripts, after handing off
  - Check: node scripts/verify.mjs

- Preserve stable public outcome names (`Completed`, `NotFound`) once
  introduced; renaming them is a human-gated decision.
- Do not add third-party runtime or test dependencies.
- Update `docs/product/todolist.md` when public behavior changes.
- Run `node scripts/verify.mjs` before claiming implementation success.
