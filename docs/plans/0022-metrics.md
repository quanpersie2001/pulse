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

| Metric | Baseline 2026-09-16 | After Phase 0 | After P1.3+P1.4+P1.5 | After Phase 1 (P1.11, 2026-09-16) | Target v3.0 |
|---|---|---|---|---|---|
| Rust lines in `src/` | 43106 | 43106 | 6517 | 10422 | < 10000 |
| Distinct error codes (exact `PulseError::{kernel,validation}` + `error.rs` codes, Python-counted) | 271 (grep-pattern count, not directly comparable) | 271 | 31 | 64 | < 40 |
| CLI leaf commands | 67 (measured; plan estimated ~60) | 67 | 13 | 20 | ≤ 22 |
| Hand-typed commands to close one Ticket | ~11 (plan estimate) | ~11 | n/a | 5 (`new`, `ready`, `run worker`, `run review`, `close`) | ≤ 6 |
| Required flags on that path | ~25 (plan estimate) | ~25 | n/a | ~2 (`--risk`, `--surface` on `new`; actor defaults from git config) | ≤ 4 |
| Friction/Ticket that is a Pulse bug | Track B: majority | Track B: majority (no new dogfood yet) | (same) | (same — no new dogfood yet; Phase 2) | < 1 |
| Repos running Pulse for real | 0 | 0 | 0 | 0 | 1 (UI + API) |

Baseline test suite (`cargo test --all-targets`) at the Phase 0 commit: 12
test binaries, 624 tests, 0 failures.

After P1.3+P1.4+P1.5 (commit `a2d164a`): 8 test binaries, 130 tests, 0
failures — deletion landed in one commit instead of spread across P1.3-P1.5
(see that commit's message), so `src/` was transiently below both the
Phase 1 and v3.0 targets before P1.6-P1.10 added back `evidence::receipt`,
`kernel::{completion,packet,checkpoint,profile,lane,reservation,run}` and
`kernel::init`'s full asset set.

**After Phase 1 (P1.11, commits through the error-code consolidation just
before this one):** 9 test binaries, 193 tests, 0 failures
(`cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` /
`cargo test --all-targets` all green). `src/` = 10422 lines, under the
Phase 1 gate (< 12000) but over the v3.0 target (< 10000) by 422 lines —
expected: P2.1 (`learn/*`, `docs::{applicable,check}`) and P3 (`board`,
`doctor`) still land. 64 error codes is over both the Phase 1 gate (< 60,
by 4) and the v3.0 target (< 40); of the 64, 21 are pre-existing
`storage`/`error.rs`/`runner::` codes this plan's file-fate table marks
"giữ nguyên" (`io_error`, `json_error`, `cas_conflict`, `runner_spec_invalid`,
etc.) and were never in scope to remove — the ~43 codes v3 actually
introduced are already under the final target on their own. A P1.11 pass
merged 4 near-duplicate new codes (`checkpoint_input_invalid` +
`handoff_input_invalid` -> `from_file_invalid`; `lane_output_missing` ->
`lane_output_invalid`; `ready_gate_failed` -> `gate_failed`) before this
count; further squeezing the new codes down risked losing which condition
failed for marginal budget gain, so this was not pursued further this
session. 20 CLI leaves and a 5-command/~2-flag golden-path-so-far (`new` ->
`ready` -> `run worker` -> `run review` -> `close`; a human never touches
`checkpoint`/`handoff`, the worker calls those itself) both already meet
the v3.0 target, though only against the fixture-fake-agent tests in this
repo — no dogfood target exists yet to confirm it end-to-end (Phase 2).
