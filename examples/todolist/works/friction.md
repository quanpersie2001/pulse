# Friction log — Track B dogfood

Every entry is a real friction hit while running Tickets TK-003..TK-008
through Pulse. One line each: what hurt, where, and what would fix it.
Feeds the v0.2 backlog (PRODUCT.md §11) after the round.

## 2026-09-06 (shaping phase)

- `work sync` rejects any wrapped line under `## Open questions` with
  `ticket_brief_open_question_disposition_missing`. The error names the
  disposition rule, but the actual problem is the continuation line of a
  wrapped bullet. Cost: one failed sync per ticket (6/6 failed on first
  sync). Fix shape: parser should skip/merge continuation lines, or the
  error should point at the offending line. **CLOSED 2026-09-07.** Both
  halves done: a non-bullet line now continues the question above it, and
  every open-question error quotes the offending line. Fixing only the
  reported symptom would have left the same defect silent elsewhere —
  `list_section`, `parse_acceptance` and `parse_key_values` all dropped
  the second half of a wrapped bullet with no error at all, so a wrapped
  `AC-1:` summary or `Rationale:` silently lost half its text. All three
  now fold continuation lines. Cover:
  `src/graph/model/brief.rs::tests::{a_wrapped_open_question_continues_the_bullet_above_it,
  open_question_errors_name_the_offending_line,
  wrapped_bullets_keep_their_second_half_in_every_list_section}`.
- `work ready --profile` looks like it takes a PULSE.md verification
  profile (`module-change`); it actually wants a readiness profile and
  only `contract_readiness` exists. The flag collides with a documented
  concept that lives elsewhere. Fix shape: rename the flag or support the
  verification profile namespace it implies.
- `authority.json` must be byte-canonical (sorted keys, pretty, exact
  trailing newline) or every readiness check reports
  `readiness_policy_invalid` with no hint about canonicality. Nothing in
  the CLI writes or repairs this file; it drifted by ordinary editing.
  Fix shape: `pulse policy normalize` command, or a reason code that says
  "not canonical" (the code exists: `readiness_policy_not_canonical` —
  it just never surfaced to me).
- Actor syntax is implicit: `--actor quanpersie2001` silently becomes
  `system:quanpersie2001` and fails grant checks; the working form is
  `--actor human:quanpersie2001`. The error shows the normalized actor,
  which is how I guessed the prefix. Fix shape: parse `kind:id` strictly
  and reject bare ids, or default bare ids to the developer principal.
- `draft -> shaped` needs `--reason-code` AND `--reason` together but the
  first attempt with only `--reason` fails with `missing_status_reason`
  after I already spent a revision bump cycle on `--expected-revision`.
  Friction is small but the flag interdependency is undocumented.
- `work ready <id>` is a pure readiness report; it does not dispatch the
  Ticket. The actual `ready` status needs a separate
  `work transition --to ready` with `--expected-revision` + reason pair.
  The natural command reports, the mutation hides behind `transition`.
- Core bug (fixed in f7acd8d): a reviewer re-running `work verify` with a
  fresh idempotency key seals a second passed receipt; close then refused
  with `close_verification_ambiguous` even though both receipts were from
  the same actor on the same handoff.
- Core bug (fixed in 074fabf): receipts sealed before the A1/A2 envelope
  fields existed failed fingerprint validation, so THREE old verification
  receipts and FIVE old handoff receipts blocked every new packet build
  with `verification_fingerprint_mismatch` — a fresh `work packet` on a
  brand-new ticket died on year-old evidence.
- Reviewer agents probe: the claude reviewer recorded a placeholder
  verification receipt while calibrating its own tooling
  (`idempotency-key` suffix `-test3`), then recorded the real verdict.
  Receipt hygiene for agent roles (dry-run mode? probe receipts?) is a
  v0.2 question.
- `docs validate --record` without `--actor` fails with
  `docs_validation_actor_required` only after validation work is done;
  the reviewer agent hit this and almost skipped recording.

## 2026-09-06 (TK-004 rework cycle)

- THE rework gap, now fixed (c3778da): after the reviewer's rework
  verdict, `pulse run worker` silently resumed the OLD lease with the
  OLD packet — the worker would never see the findings. Worse, two of
  my monitoring attempts spawned codex into that stale-packet run and I
  killed them mid-flight. There was no way to tell a re-dispatch from a
  zombie resume from the outside.
- The reviewer's rework verdict summary (~1100 chars) was copied
  verbatim into the node's `status_reason`, which validates at ≤500 —
  every graph read then refused with `invalid_status_reason`. The
  ticket was unreadable until hand-repaired. Writer now bounds it
  (receipt keeps the full text).
- Packet schema said `rework: [string]` while the code emitted objects:
  the first packet build with a real observation died on the embedded
  schema. Track A added the field in code but missed the schema.
- Whose job is the docs receipt? TK-003's reviewer ran
  `docs validate --record` itself and passed the worker; TK-004's
  reviewer rework'd the worker for missing it. Both defensible — the
  contract is ambiguous. **CLOSED 2026-09-07 by Decision 0016.** The
  ambiguity turned out to have three layers, not one: (1) the worker
  prompt never mentioned the receipt while the reviewer prompt did;
  (2) `handoff --evidence-receipt` was a dead end — the close gate reads
  only the reviewer's proofs, so a worker doing it right still did not
  satisfy close; (3) `reviewer-input.json` filtered docs receipts by
  ticket id, but they are subject-bound to the docs registry, so that
  list was empty for every Ticket ever run while 16 such receipts
  existed. Fixed by giving the worker ownership, refusing the handoff
  without the receipt (`handoff_documentation_receipt_missing`), and
  filtering the reviewer's list by source commit. Cover:
  `tests/graph/documentation_handoff.rs`,
  `tests/runner/reviewer.rs::reviewer_input_lists_docs_receipts_bound_to_the_reviewed_commit`.
- The rework finding shape worked exactly as designed once visible:
  check + owner + severity, worker fixed, reviewer re-ran the check.
  That half of the loop is solid.

## 2026-09-07 (TK-005 R2 cycle)

- Core bug (fixed in e27b5e4): `work handoff --evidence-receipt`
  deadlocked against itself — the handoff holds the write fence while
  receipt verification loads the docs registry, whose read also takes
  the fence. The worker had to record the docs receipt WITHOUT
  referencing it, and the next reviewer then rework'd on the missing
  proof chain. Two defects chaining into a false-negative rework.
- Worktree dispatch gap (the big one) — CLOSED 2026-09-07 by Decision
  0015: the run workspace (worker-prompt.md, worker-input.json) was
  written only to the main repo's runtime, so a worktree worker found no
  prompt at its cwd and wandered into the main checkout through packet
  absolute paths. TK-006 still succeeded (the fence bound the clean
  worktree), but the isolation guarantee was accidental, not enforced.
  Fixed by mirroring the run workspace into the worktree, embedding the
  absolute workspace path in the prompt, and mapping worktree→main for
  state planes at the CLI boundary (`.pulse-owned` marker plus Git
  corroboration). Regression cover: `tests/runner/worktree_dispatch.rs`,
  whose fake agent resolves everything from cwd and never passes
  `--repo-root` — the exact shape that failed here.
- Cross-ticket poisoning via that gap — CLOSED 2026-09-07 by the same
  decision: TK-006's worker wrote into the shared checkout mid-flight and
  staled TK-007's proof fence, costing a full release→re-run cycle of
  TK-007. Concurrent dispatch is only as safe as the weakest path
  isolation; `worker_finds_its_run_workspace_under_its_own_cwd` now
  asserts the main checkout's dirty identity is unmoved by an isolated
  run.
- Reviewer reviewed a different tree than the worker handed off (found
  while writing 0015, never hit in anger) — CLOSED 2026-09-07:
  `decide_workspace` only applied to the worker, so reviewer and qa always
  ran in the main checkout. Had the worktree gap above been fixed alone,
  every isolated Ticket would have been reviewed against a tree without
  the worker's changes.
- Reviewer run records capture stderr_tail only — when a reviewer
  fails its `work verify` call, the error text is lost and the operator
  sees just `malformed_output`/`unproven_claim`. Bounded stdout tail in
  the run record would have saved two diagnosis round-trips.
- `close-story` requires a clean tree (`story_close_source_dirty`), so
  the developer must commit Ticket work before closing the Story — but
  the docs never sequence this. Learned by hitting it.
- The dirty fence is tree-wide: I (the OPERATOR) edited
  `works/friction.md` — an unrelated tracked prose file — between
  reviewer verification and close, and close refused with
  `close_source_stale`. LRN-001 applies to the operator seat too, not
  just agents. I reverted the file, closed, restored it. Friction: the
  error names neither the offending path nor the last-clean identity;
  a `git status`-style hint ("works/friction.md changed since handoff")
  would have saved a revert dance. Deeper question for v0.2: should
  docs-alongside-code note files fence-block a close at all?

## 2026-09-07 (Decision 0010 migration)

- One receipt Pulse could not decode killed the whole listing.
  `list_receipts` parses every file in `.pulse/evidence/receipts/` and
  returned the first parse error, so the seven pre-0010 `qa_checkpoint`
  receipts made `pulse evidence receipt list` fail for the entire
  repository — no receipt of any kind was readable through the CLI.
  Cost: the seven were found by accident while migrating, not by any
  gate. Worse, both `list_receipts` callsites in `kernel/run.rs` wrapped
  it in `unwrap_or_default()`, so the same condition made the reviewer's
  `proof_receipts` **silently empty** instead of failing — a reviewer
  would read "no qa_checkpoint exists" and rework a worker who recorded
  one correctly. That is the identical shape Decision 0016 closed, where
  an always-empty docs list cost three rework cycles.
  **CLOSED 2026-09-07.** `list_receipts` now reports undecodable files in
  `unreadable[] {id, path, reason}` and keeps listing the rest; filters
  never hide them, because the kind and subject are exactly what cannot
  be read. The reviewer input carries the same list under
  `proof_receipts.unreadable` and the reviewer prompt says what it means:
  an empty proof list is "none exists" only when `unreadable` is empty
  too, otherwise report it as a finding owned by the evidence store
  rather than reworking the worker. Real errors (an unreadable receipts
  directory) now propagate instead of becoming an empty list. Cover:
  `tests/runner/reviewer.rs::unreadable_receipt_reaches_the_reviewer_instead_of_emptying_its_proof_list`.
  Open question for v0.2: a `pulse evidence receipt list` that succeeds
  while naming unreadable files is right for a listing, but nothing yet
  *fails* on a corrupt store. A `doctor`-style check probably owns that.
