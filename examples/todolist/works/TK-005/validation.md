# TK-005 validation

Recorded by `agent:runner:worker` at handoff on source commit `e27b5e4`
(rework attempt after `verify_15189ebe790ef10b56a0711c1d`).

## Rework resolution

The reviewer's only finding (`docs-proof-audit`, high) was that the
required `documentation_validation` receipt was not referenced from the
handoff proof envelope. The first handoff could not reference it because
`work handoff --evidence-receipt` self-deadlocked on the repository fence;
that harness bug is fixed at `e27b5e4` ("verify handoff evidence receipts
before taking the fence"). No implementation change was needed: this
attempt re-runs every check on the unchanged tree, records a fresh docs
receipt bound to the current head, and references it from the handoff's
top-level evidence and from every acceptance proof.

## Checks run

| Check | Command | Result |
|---|---|---|
| verify | `node scripts/verify.mjs` | exit 0, 27 tests, 27 pass, 0 fail |
| due-tests | `node --test --test-name-pattern due test/todolist.test.mjs` | exit 0, 5 tests pass |
| qa-run | `node scripts/qa-run.mjs .pulse/runtime/run/TK-005/qa-input.json` | exit 0; QA-003 passed, QA-004 passed |
| docs-validate | `pulse docs validate --record --actor agent:runner:worker --json` | exit 0; receipt `rcpt_01M1W19322NWRV1QKB3MW3SBH9` (binds `docs/product/todolist.md` at `sha256:d7d522…` on `e27b5e4`) |

Scratch-directory CLI walk-through from the first attempt (not a recorded
check, reproduced by the AC-1/AC-3 tests):

- `add t1 one --due 12/01/2026` → stderr `error: due must be a YYYY-MM-DD
  calendar date`, exit 2, no `.todolist.json` created.
- `add t1 one --due 2026-02-30` → same error line, exit 2, no file.
- `add t1 one --due 2026-12-01` → exit 0, state has `"due": "2026-12-01"`.
- `add t2 two` → exit 0, `t2` entry has no `due` key.
- `add t3 three --due 2026-04-31` on that state → error, exit 2, state file
  byte-identical to before.

## Acceptance mapping

Every acceptance proof also references the docs receipt
`rcpt_01M1W19322NWRV1QKB3MW3SBH9`, because the behavior doc is a required
document for this Ticket and each AC describes user-visible behavior the
doc now states.

- AC-1 (`add --due` stores verbatim, survives roundtrip, `list` third
  column): verify, due-tests — test "add --due persists the date and list
  shows it as a third column"; qa-run — QA-003.
- AC-2 (undated add and pre-due state files byte-identical): verify,
  due-tests — test "undated add and pre-due state files behave
  byte-identically" plus every pre-existing CLI test still passing.
- AC-3 (`12/01/2026` and `2026-02-30` → one `error:` line, exit 2, nothing
  written): verify, due-tests — test "add with an invalid --due prints one
  error line, exits 2 and writes nothing"; qa-run — QA-004.

## Remaining risk

- The usage line now mentions `[--due <YYYY-MM-DD>]`; the usage error text
  is therefore not byte-identical to before. Todo output for undated todos
  is unchanged, which is what AC-2 and the invariant pin.
- Year `0000` passes the shape check; no acceptance criterion constrains the
  year range and the Story approach does not either.
- The QA runner handlers exercise only the pure surface, matching the
  baseline's `surface: api`; the CLI path is covered by the test suite.
- Harness: the reviewer packet's `proof_receipts.documentation_validation`
  is built by filtering receipts whose subject id equals the Ticket id, but
  docs receipts carry the docs registry as their subject, so that list stays
  empty even when the handoff references the receipt. Reviewers should read
  the receipt ids from `handoffs[].acceptance_proofs` instead.
