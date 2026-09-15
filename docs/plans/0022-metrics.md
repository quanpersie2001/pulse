# Plan 0022 metrics

Measured per §2 of `docs/plans/0022-thin-harness.md`. Re-measure at the end
of every phase; add a column, don't overwrite the previous one.

## How each row is measured

```bash
find src -name '*.rs' | xargs cat | wc -l
grep -rhoE '"[a-z_]+_(missing|stale|required|violation|gap|conflict|mismatch|invalid|unchanged|denied|exhausted|drift|torn_tail)"' src | sort -u | wc -l
# leaf CLI commands: recurse `pulse <path> --help` until a subcommand's
# --help has no "Commands:" section; count those leaves (see history for the
# exact recursive script — bash only, zsh does not word-split unquoted args).
```

The last three rows (hand-typed commands to close a Ticket, required flags
on that path, friction-per-Ticket that is a Pulse bug) are not
mechanically measured; they are carried from the plan's own baseline
estimate and re-judged qualitatively once the dogfood target exists in
Phase 2.

## Table

| Metric | Baseline 2026-09-16 | After Phase 0 | After P1.3+P1.4+P1.5 (2026-09-16) | Target v3.0 |
|---|---|---|---|---|
| Rust lines in `src/` | 43106 | 43106 | 6517 | < 10000 |
| Distinct error codes (pattern match) | 271 | 271 | 31 | < 40 |
| CLI leaf commands | 67 (measured; plan estimated ~60) | 67 | 13 | ≤ 22 |
| Hand-typed commands to close one Ticket | ~11 (plan estimate) | ~11 | n/a — `close` doesn't exist until P1.7 | ≤ 6 |
| Required flags on that path | ~25 (plan estimate) | ~25 | n/a | ≤ 4 |
| Friction/Ticket that is a Pulse bug | Track B: majority | Track B: majority (no new dogfood yet) | Track B: majority (no new dogfood yet) | < 1 |
| Repos running Pulse for real | 0 | 0 | 0 | 1 (UI + API) |

Baseline test suite (`cargo test --all-targets`) at the Phase 0 commit: 12
test binaries, 624 tests, 0 failures.

After P1.3+P1.4+P1.5 (commit `a2d164a`): 8 test binaries, 130 tests, 0
failures. `src/` is already under the Phase 1 target (< 12000) and the v3.0
target (< 10000) because P1.3/P1.4/P1.5 had to merge into one commit (see
that commit's message) — deletion landed all at once instead of spread
across P1.3-P1.5. The count will grow again as P1.6-P1.10 add
`evidence::receipt`, `kernel::{completion,packet,checkpoint,profile,lane,
run}`, and `kernel::init`'s full asset set; 13 CLI leaves will grow the
same way (`checkpoint`, `handoff`, `release`, `close`, `close-story`,
`packet`, `run` are not implemented yet).
